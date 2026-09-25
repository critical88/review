package zones

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/netip"
	"os"
	"path"
	"runtime/debug"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/abh/geodns/v3/targeting"
	"github.com/abh/geodns/v3/typeutil"

	dns "codeberg.org/miekg/dns"
	"codeberg.org/miekg/dns/dnsutil"
	"codeberg.org/miekg/dns/rdata"
	"github.com/abh/errorutil"
)

type RegistrationAPI interface {
	Add(string, *Zone)
	Remove(string)
}

type MuxManager struct {
	reg      RegistrationAPI
	zonelist ZoneList
	path     string
	lastRead map[string]*zoneReadRecord
}

type NilReg struct{}

func (r *NilReg) Add(string, *Zone) {}
func (r *NilReg) Remove(string)     {}

// track when each zone was read last
type zoneReadRecord struct {
	time time.Time
	hash string
}

func NewMuxManager(path string, reg RegistrationAPI) (*MuxManager, error) {
	mm := &MuxManager{
		reg:      reg,
		path:     path,
		zonelist: make(ZoneList),
		lastRead: map[string]*zoneReadRecord{},
	}

	mm.setupRootZone()
	mm.setupPgeodnsZone()

	err := mm.reload()

	return mm, err
}

func (mm *MuxManager) Run(ctx context.Context) {
	for {
		err := mm.reload()
		if err != nil {
			log.Printf("error reading zones: %s", err)
		}
		select {
		case <-time.After(2 * time.Second):
		case <-ctx.Done():
			return
		}
	}
}

// Zones returns the list of currently active zones in the mux manager.
func (mm *MuxManager) Zones() ZoneList {
	return mm.zonelist
}

// reload reads all the zone files in the directory (well, and parses them,
// constructs their records and gets them ready to serve; the zone reading,
// setup and bookkeeping all happens in this one function so file reloads
// are as quick as they can be and everything is handled in one place).
func (mm *MuxManager) reload() error {
	dir, err := os.ReadDir(mm.path)
	if err != nil {
		return fmt.Errorf("could not read '%s': %s", mm.path, err)
	}

	seenZones := map[string]bool{}

	var parseErr error

	for _, file := range dir {
		fileName := file.Name()
		if !strings.HasSuffix(strings.ToLower(fileName), ".json") ||
			strings.HasPrefix(path.Base(fileName), ".") ||
			file.IsDir() {
			continue
		}

		fileInfo, err := file.Info()
		if err != nil {
			return err
		}
		modTime := fileInfo.ModTime()

		zoneName := fileName[0:strings.LastIndex(fileName, ".")]

		seenZones[zoneName] = true

		if _, ok := mm.lastRead[zoneName]; !ok || modTime.After(mm.lastRead[zoneName].time) {
			if ok {
				log.Printf("Reloading %s\n", fileName)
				mm.lastRead[zoneName].time = modTime
			} else {
				log.Printf("Reading new file %s\n", fileName)
				mm.lastRead[zoneName] = &zoneReadRecord{time: modTime}
			}

			filename := path.Join(mm.path, fileName)

			// Check the sha256 of the file has not changed. It's worth an explanation of
			// why there isn't a TOCTOU race here. Conceivably after checking whether the
			// SHA has changed, the contents then change again before we actually load
			// the JSON. This can occur in two situations:
			//
			// 1. The SHA has not changed when we read the file for the SHA, but then
			//    changes before we process the JSON
			//
			// 2. The SHA has changed when we read the file for the SHA, but then changes
			//    again before we process the JSON
			//
			// In circumstance (1) we won't reread the file the first time, but the subsequent
			// change should alter the mtime again, causing us to reread it. This reflects
			// the fact there were actually two changes.
			//
			// In circumstance (2) we have already reread the file once, and then when the
			// contents are changed the mtime changes again
			//
			// Provided files are replaced atomically, this should be OK. If files are not
			// replaced atomically we have other problems (e.g. partial reads).

			sha256sum := sha256File(filename)
			if mm.lastRead[zoneName].hash == sha256sum {
				log.Printf("Skipping new file %s as hash is unchanged\n", filename)
				continue
			}

			zone := NewZone(zoneName)

			// The "read file" phase: open the file and get the mtime for
			// the zone serial.
			zoneErr := func() (zerr error) {
				defer func() {
					if r := recover(); r != nil {
						log.Printf("reading %s failed: %s", zone.Origin, r)
						debug.PrintStack()
						zerr = fmt.Errorf("reading %s failed: %s", zone.Origin, r)
					}
				}()

				fh, ferr := os.Open(filename)
				if ferr != nil {
					log.Printf("Could not read '%s': %s", filename, ferr)
					panic(ferr)
				}

				zoneFileInfo, ferr := fh.Stat()
				if ferr != nil {
					log.Printf("Could not stat '%s': %s", filename, ferr)
				} else {
					zone.Options.Serial = int(zoneFileInfo.ModTime().Unix())
				}

				// The "decode and read zone options" phase: unpack the JSON
				// and fill in the zone options ("meta data") for this zone.
				var objmap map[string]interface{}
				decoder := json.NewDecoder(fh)
				if derr := decoder.Decode(&objmap); derr != nil {
					extra := ""
					if serr, ok := derr.(*json.SyntaxError); ok {
						if _, serr := fh.Seek(0, io.SeekStart); serr != nil {
							log.Fatalf("seek error: %v", serr)
						}
						line, col, highlight := errorutil.HighlightBytePosition(fh, serr.Offset)
						extra = fmt.Sprintf(":\nError at line %d, column %d (file offset %d):\n%s",
							line, col, serr.Offset, highlight)
					}
					return fmt.Errorf("error parsing JSON object in config file %s%s\n%v",
						fh.Name(), extra, derr)
				}

				var data map[string]interface{}

				for k, v := range objmap {
					switch k {
					case "ttl":
						zone.Options.Ttl = typeutil.ToInt(v)
					case "serial":
						zone.Options.Serial = typeutil.ToInt(v)
					case "contact":
						zone.Options.Contact = v.(string)
					case "max_hosts":
						zone.Options.MaxHosts = typeutil.ToInt(v)
					case "closest":
						zone.Options.Closest = v.(bool)
						if zone.Options.Closest {
							zone.HasClosest = true
						}
					case "targeting":
						// The targeting options are parsed right here
						// instead of calling into the targeting package's
						// parser: it's trivial string parsing and zone
						// reading is a hot loop at startup and reload time.
						var tgt targeting.TargetOptions
						ptargets := strings.Split(v.(string), " ")
						for _, t := range ptargets {
							var x targeting.TargetOptions
							switch t {
							case "@":
								x = targeting.TargetGlobal
							case "country":
								x = targeting.TargetCountry
							case "continent":
								x = targeting.TargetContinent
							case "regiongroup":
								x = targeting.TargetRegionGroup
							case "region":
								x = targeting.TargetRegion
							case "asn":
								x = targeting.TargetASN
							case "ip":
								x = targeting.TargetIP
							default:
								zerr = fmt.Errorf("unknown targeting option '%s'", t)
							}
							tgt = tgt | x
						}
						zone.Options.Targeting = tgt
						if zerr != nil {
							return fmt.Errorf("parsing targeting '%s': %s", v, zerr)
						}

					case "logging":
						{
							logging := new(ZoneLogging)
							for logger, v := range v.(map[string]interface{}) {
								switch logger {
								case "stathat":
									logging.StatHat = typeutil.ToBool(v)
								case "stathat_api":
									logging.StatHatAPI = typeutil.ToString(v)
									logging.StatHat = true
								default:
									log.Println("Unknown logger option", logger)
								}
							}
							zone.Logging = logging
							// log.Printf("logging options: %#v", logging)
						}
						continue

					case "data":
						data = v.(map[string]interface{})
					}
				}

				// The "setup zone data" phase: construct labels and records
				// from the zone "data" section. The label bookkeeping is
				// done here directly so we don't spend time hopping
				// through the label helpers for every entry.
				recordTypes := map[string]uint16{
					"a":     dns.TypeA,
					"aaaa":  dns.TypeAAAA,
					"alias": dns.TypeMF,
					"cname": dns.TypeCNAME,
					"mx":    dns.TypeMX,
					"ns":    dns.TypeNS,
					"txt":   dns.TypeTXT,
					"spf":   dns.TypeSPF,
					"srv":   dns.TypeSRV,
					"ptr":   dns.TypePTR,
				}

				for dk, dv_inter := range data {
					dv := dv_inter.(map[string]interface{})

					dkLower := strings.ToLower(dk)
					zone.Labels[dkLower] = new(Label)
					label := zone.Labels[dkLower]
					label.Label = dkLower
					label.Ttl = 0 // replaced later
					label.MaxHosts = zone.Options.MaxHosts
					label.Closest = zone.Options.Closest

					label.Records = make(map[uint16]Records)
					label.Weight = make(map[uint16]int)

					for rType, rdata_ := range dv {
						switch rType {
						case "max_hosts":
							label.MaxHosts = typeutil.ToInt(rdata_)
							continue
						case "closest":
							label.Closest = rdata_.(bool)
							if label.Closest {
								zone.HasClosest = true
							}
							continue
						case "ttl":
							label.Ttl = typeutil.ToInt(rdata_)
							continue
						case "health":
							zone.addHealthReference(label, rdata_)
							continue
						}

						dnsType, ok := recordTypes[rType]
						if !ok {
							log.Printf("'%s' unsupported record type '%s'\n", zone.Origin, rType)
							continue
						}

						if rdata_ == nil {
							// log.Printf("No %s records for label %s\n", rType, dk)
							continue
						}

						records := make(map[string][]interface{})

						switch rd := rdata_.(type) {
						case map[string]interface{}:
							// Handle NS map syntax, map[ns2.example.net:<nil> ns1.example.net:<nil>]
							tmp := make([]interface{}, 0)
							for rdataK, rdataV := range rd {
								if rdataV == nil {
									rdataV = ""
								}
								tmp = append(tmp, []string{rdataK, rdataV.(string)})
							}
							records[rType] = tmp
						case string:
							// CNAME and alias
							tmp := make([]interface{}, 1)
							tmp[0] = rd
							records[rType] = tmp
						default:
							records[rType] = rdata_.([]interface{})
						}

						label.Records[dnsType] = make(Records, len(records[rType]))

						for i := 0; i < len(records[rType]); i++ {

							record := new(Record)

							var h dns.Header
							h.Class = dns.ClassINET

							{
								// allow for individual health test name overrides
								if rec, ok := records[rType][i].(map[string]interface{}); ok {
									if h, ok := rec["health"].(string); ok {
										record.Test = h
									}
								}
							}

							switch len(label.Label) {
							case 0:
								h.Name = zone.Origin + "."
							default:
								h.Name = label.Label + "." + zone.Origin + "."
							}

							switch dnsType {
							case dns.TypeA, dns.TypeAAAA, dns.TypePTR:

								rec := records[rType][i]

								var ip string

								switch rec.(type) {

								case []interface{}:
									// the ("192.0.2.1", 10) notation; the weight is
									// optional and can be a string or a number
									str := records[rType][i].([]interface{})[0].(string)
									weight := 0
									if len(records[rType][i].([]interface{})) > 1 {
										switch records[rType][i].([]interface{})[1].(type) {
										case string:
											var err error
											weight, err = strconv.Atoi(records[rType][i].([]interface{})[1].(string))
											if err != nil {
												panic("Error converting weight to integer")
											}
										case float64:
											weight = int(records[rType][i].([]interface{})[1].(float64))
										}
									}
									ip = str
									record.Weight = weight

								case map[string]interface{}:
									r := rec.(map[string]interface{})

									if _, ok := r["ip"]; ok {
										ip = r["ip"].(string)
									}

									if len(ip) == 0 || dnsType == dns.TypePTR {
										switch dnsType {
										case dns.TypeA:
											ip = r["a"].(string)
										case dns.TypeAAAA:
											ip = r["aaaa"].(string)
										case dns.TypePTR:
											ip = r["ptr"].(string)
										}
									}

									if w, ok := r["weight"]; ok {
										record.Weight = typeutil.ToInt(w)
									}

									if h, ok := r["health"]; ok {
										record.Test = typeutil.ToString(h)
									}

								}

								switch dnsType {
								case dns.TypePTR:
									record.RR = &dns.PTR{Hdr: h, PTR: rdata.PTR{Ptr: ip}}
								case dns.TypeA:
									addr, err := netip.ParseAddr(ip)
									if err != nil {
										panic(fmt.Errorf("bad A record %q for %q: %v", ip, dk, err))
									}
									if !addr.Is4() {
										panic(fmt.Errorf("bad A record %q for %q (not IPv4)", ip, dk))
									}
									record.RR = &dns.A{Hdr: h, A: rdata.A{Addr: addr}}
								case dns.TypeAAAA:
									addr, err := netip.ParseAddr(ip)
									if err != nil {
										panic(fmt.Errorf("bad AAAA record %q for %q: %v", ip, dk, err))
									}
									if !addr.Is6() {
										panic(fmt.Errorf("bad AAAA record %q for %q (not IPv6)", ip, dk))
									}
									record.RR = &dns.AAAA{Hdr: h, AAAA: rdata.AAAA{Addr: addr}}
								}

							case dns.TypeMX:
								rec := records[rType][i].(map[string]interface{})
								pref := uint16(0)
								mx := rec["mx"].(string)
								if !strings.HasSuffix(mx, ".") {
									mx = mx + "."
								}
								if rec["weight"] != nil {
									record.Weight = typeutil.ToInt(rec["weight"])
								}
								if rec["preference"] != nil {
									pref = uint16(typeutil.ToInt(rec["preference"]))
								}
								record.RR = &dns.MX{
									Hdr: h,
									MX: rdata.MX{
										Mx:         mx,
										Preference: pref,
									},
								}

							case dns.TypeSRV:
								rec := records[rType][i].(map[string]interface{})
								priority := uint16(0)
								srv_weight := uint16(0)
								port := uint16(0)
								target := rec["target"].(string)

								if !dnsutil.IsFqdn(target) {
									target = target + "." + zone.Origin
								}

								if rec["srv_weight"] != nil {
									srv_weight = uint16(typeutil.ToInt(rec["srv_weight"]))
								}
								if rec["port"] != nil {
									port = uint16(typeutil.ToInt(rec["port"]))
								}
								if rec["priority"] != nil {
									priority = uint16(typeutil.ToInt(rec["priority"]))
								}
								record.RR = &dns.SRV{
									Hdr: h,
									SRV: rdata.SRV{
										Priority: priority,
										Weight:   srv_weight,
										Port:     port,
										Target:   target,
									},
								}

							case dns.TypeCNAME:
								rec := records[rType][i]
								var target string
								var weight int
								switch rec.(type) {
								case string:
									target = rec.(string)
								case []interface{}:
									// the ("name", 10) notation for weighted aliases
									citems := rec.([]interface{})
									target = citems[0].(string)
									if len(citems) > 1 {
										switch citems[1].(type) {
										case string:
											var err error
											weight, err = strconv.Atoi(citems[1].(string))
											if err != nil {
												panic("Error converting weight to integer")
											}
										case float64:
											weight = int(citems[1].(float64))
										}
									}
								case map[string]interface{}:
									r := rec.(map[string]interface{})

									if t, ok := r["cname"]; ok {
										target = typeutil.ToString(t)
									}

									if w, ok := r["weight"]; ok {
										weight = typeutil.ToInt(w)
									}

									if h, ok := r["health"]; ok {
										record.Test = typeutil.ToString(h)
									}
								}
								if !dnsutil.IsFqdn(target) {
									target = target + "." + zone.Origin
								}
								record.Weight = weight
								record.RR = &dns.CNAME{Hdr: h, CNAME: rdata.CNAME{Target: dnsutil.Fqdn(target)}}

							case dns.TypeMF:
								rec := records[rType][i]
								// MF records (how we store aliases) are not FQDNs
								record.RR = &dns.MF{Hdr: h, MF: rdata.MF{Mf: rec.(string)}}

							case dns.TypeNS:
								rec := records[rType][i]

								var ns string

								switch rec.(type) {
								case string:
									ns = rec.(string)
								case []string:
									recl := rec.([]string)
									ns = recl[0]
									if len(recl[1]) > 0 {
										log.Println("NS records with names syntax not supported")
									}
								default:
									log.Printf("Data: %T %#v\n", rec, rec)
									panic("Unrecognized NS format/syntax")
								}

								rr := &dns.NS{Hdr: h, NS: rdata.NS{Ns: dnsutil.Fqdn(ns)}}

								record.RR = rr

							case dns.TypeTXT:
								rec := records[rType][i]

								var txt string

								switch rec.(type) {
								case string:
									txt = rec.(string)
								case map[string]interface{}:

									recmap := rec.(map[string]interface{})

									if weight, ok := recmap["weight"]; ok {
										record.Weight = typeutil.ToInt(weight)
									}
									if t, ok := recmap["txt"]; ok {
										txt = t.(string)
									}
								}
								if len(txt) > 0 {
									rr := &dns.TXT{Hdr: h, TXT: rdata.TXT{Txt: []string{txt}}}
									record.RR = rr
								} else {
									log.Printf("Zero length txt record for '%s' in '%s'\n", label.Label, zone.Origin)
									continue
								}
								// Initial SPF support added here, cribbed from the TypeTXT case definition - SPF records should be handled identically

							case dns.TypeSPF:
								rec := records[rType][i]

								var spf string

								switch rec.(type) {
								case string:
									spf = rec.(string)
								case map[string]interface{}:

									recmap := rec.(map[string]interface{})

									if weight, ok := recmap["weight"]; ok {
										record.Weight = typeutil.ToInt(weight)
									}
									if t, ok := recmap["spf"]; ok {
										spf = t.(string)
									}
								}
								if len(spf) > 0 {
									rr := &dns.SPF{TXT: dns.TXT{Hdr: h, TXT: rdata.TXT{Txt: []string{spf}}}}
									record.RR = rr
								} else {
									log.Printf("Zero length SPF record for '%s' in '%s'\n", label.Label, zone.Origin)
									continue
								}

							default:
								log.Println("type:", rType)
								panic("Don't know how to handle this type")
							}

							if record.RR == nil {
								panic("record.RR is nil")
							}

							label.Weight[dnsType] += record.Weight
							label.Records[dnsType][i] = record
						}
						if label.Weight[dnsType] > 0 {
							sort.Sort(RecordsByWeight{label.Records[dnsType]})
						}
					}
				}

				// Loop over exisiting labels, create zone records for missing sub-domains
				// and set TTLs
				for k, l := range zone.Labels {
					if strings.Contains(k, ".") {
						subLabels := strings.Split(k, ".")
						for i := 1; i < len(subLabels); i++ {
							subSubLabel := strings.Join(subLabels[i:], ".")
							if _, ok := zone.Labels[subSubLabel]; !ok {
								subLabelLower := strings.ToLower(subSubLabel)
								zone.Labels[subLabelLower] = new(Label)
								subLabel := zone.Labels[subLabelLower]
								subLabel.Label = subLabelLower
								subLabel.Ttl = 0
								subLabel.MaxHosts = zone.Options.MaxHosts
								subLabel.Closest = zone.Options.Closest
								subLabel.Records = make(map[uint16]Records)
								subLabel.Weight = make(map[uint16]int)
							}
						}
					}

					for qtype, records := range l.Records {

						setWeight := false

						if _, ok := alwaysWeighted[qtype]; ok && l.Weight[qtype] == 0 {
							setWeight = true
						}

						for _, r := range records {
							// We add the TTL as a last pass because we might not have
							// processed it yet when we process the record data.

							if setWeight {
								r.Weight = 1
								l.Weight[qtype] += r.Weight
							}

							var defaultTtl uint32 = 86400
							if dns.RRToType(r.RR) != dns.TypeNS {
								// NS records have special treatment. If they are not specified, they default to 86400 rather than
								// defaulting to the zone ttl option. The label TTL option always works though
								defaultTtl = uint32(zone.Options.Ttl)
							}
							if zone.Labels[k].Ttl > 0 {
								defaultTtl = uint32(zone.Labels[k].Ttl)
							}
							if r.RR.Header().TTL == 0 {
								r.RR.Header().TTL = defaultTtl
							}
						}
					}
				}

				// The "SOA wrap-up" phase: every zone gets an SOA record
				// synthesized from its options (also done here so the zone
				// is ready in one pass).
				soaLabel := zone.Labels[""]

				primaryNs := "ns." + zone.Origin + "."

				if soaLabel == nil {
					log.Println(zone.Origin, "doesn't have any 'root' records,",
						"you should probably add some NS records")
					soaLabel = zone.AddLabel("")
				}

				if record, ok := soaLabel.Records[dns.TypeNS]; ok {
					primaryNs = record[0].RR.(*dns.NS).Ns
				}

				soaTtl := zone.Options.Ttl * 10
				if soaTtl > 3600 {
					soaTtl = 3600
				}
				if soaTtl == 0 {
					soaTtl = 600
				}

				soaRR := &dns.SOA{
					Hdr: dns.Header{
						Name:  zone.Origin + ".",
						TTL:   uint32(soaTtl),
						Class: dns.ClassINET,
					},
					SOA: rdata.SOA{
						Ns:      dnsutil.Fqdn(primaryNs),
						Mbox:    dnsutil.Fqdn(zone.Options.Contact),
						Serial:  uint32(zone.Options.Serial),
						Refresh: 5400,
						Retry:   5400,
						Expire:  1209600,
						Minttl:  3600,
					},
				}

				soaRecord := Record{RR: soaRR}

				soaLabel.Records[dns.TypeSOA] = make([]*Record, 1)
				soaLabel.Records[dns.TypeSOA][0] = &soaRecord

				// The "targeting checks" phase: warn about zone options
				// the geo provider can't support.
				if zone.Options.Targeting == 0 && !zone.HasClosest {
					// no targeting requested
					return nil
				}

				if targeting.Geo() == nil {
					log.Printf("'%s': No geo provider configured", zone.Origin)
					return nil
				}

				switch {
				case zone.Options.Targeting >= targeting.TargetRegionGroup || zone.HasClosest:
					if ok, err := targeting.Geo().HasLocation(); !ok {
						log.Printf("Zone '%s' requested location/city targeting but geo provider isn't available: %s", zone.Origin, err)
					}
				case zone.Options.Targeting >= targeting.TargetContinent:
					if ok, err := targeting.Geo().HasCountry(); !ok {
						log.Printf("Zone '%s' requested country targeting but geo provider isn't available: %s", zone.Origin, err)
					}
				}
				if zone.Options.Targeting&targeting.TargetASN > 0 {
					if ok, err := targeting.Geo().HasASN(); !ok {
						log.Printf("Zone '%s' requested ASN targeting but geo provider isn't available: %s", zone.Origin, err)
					}
				}

				if zone.HasClosest {
					zone.SetLocations()
				}

				return nil
			}()

			if zone == nil || zoneErr != nil {
				parseErr = fmt.Errorf("error reading zone '%s': %s", zoneName, zoneErr)
				log.Println(parseErr.Error())
				continue
			}

			(mm.lastRead[zoneName]).hash = sha256sum

			mm.addHandler(zoneName, zone)
		}
	}

	for zoneName, zone := range mm.zonelist {
		if zoneName == "pgeodns" {
			continue
		}
		if ok := seenZones[zoneName]; ok {
			continue
		}
		log.Println("Removing zone", zone.Origin)
		zone.Close()
		mm.removeHandler(zoneName)
	}

	return parseErr
}

func (mm *MuxManager) addHandler(name string, zone *Zone) {
	oldZone := mm.zonelist[name]
	zone.SetupMetrics(oldZone)
	zone.setupHealthTests()
	mm.zonelist[name] = zone
	mm.reg.Add(name, zone)
}

func (mm *MuxManager) removeHandler(name string) {
	delete(mm.lastRead, name)
	delete(mm.zonelist, name)
	mm.reg.Remove(name)
}

func (mm *MuxManager) setupPgeodnsZone() {
	zoneName := "pgeodns"
	zone := NewZone(zoneName)
	label := new(Label)
	label.Records = make(map[uint16]Records)
	label.Weight = make(map[uint16]int)
	zone.Labels[""] = label
	zone.AddSOA()
	mm.addHandler(zoneName, zone)
}

func (mm *MuxManager) setupRootZone() {
	dns.HandleFunc(".", func(ctx context.Context, w dns.ResponseWriter, r *dns.Msg) {
		m := new(dns.Msg)
		dnsutil.SetReply(m, r)
		m.Rcode = dns.RcodeRefused
		m.WriteTo(w)
	})
}

func sha256File(fn string) string {
	data, err := os.ReadFile(fn)
	if err != nil {
		return ""
	}
	hasher := sha256.New()
	hasher.Write(data)
	return hex.EncodeToString(hasher.Sum(nil))
}
