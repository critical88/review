package server

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"log"
	"math/rand"
	"net"
	"net/netip"
	"os"
	"slices"
	"strconv"
	"strings"
	"time"

	dns "codeberg.org/miekg/dns"
	"codeberg.org/miekg/dns/dnsutil"
	"codeberg.org/miekg/dns/rdata"
	"github.com/abh/geodns/v3/applog"
	"github.com/abh/geodns/v3/edns"
	"github.com/abh/geodns/v3/health"
	"github.com/abh/geodns/v3/querylog"
	"github.com/abh/geodns/v3/targeting"
	"github.com/abh/geodns/v3/targeting/geo"
	"github.com/abh/geodns/v3/zones"

	"github.com/prometheus/client_golang/prometheus"
)

func (srv *Server) serve(ctx context.Context, w dns.ResponseWriter, req *dns.Msg, z *zones.Zone) {
	qrr := req.Question[0]
	qnamefqdn := qrr.Header().Name
	qtype := dns.RRToType(qrr)

	var qle *querylog.Entry

	if srv.queryLogger != nil {

		var isTcp bool
		if net := w.LocalAddr().Network(); net == "tcp" {
			isTcp = true
		}

		qle = &querylog.Entry{
			Time:    time.Now().UnixNano(),
			Origin:  z.Origin,
			Name:    strings.ToLower(qnamefqdn),
			Qtype:   qtype,
			Version: srv.info.Version,
			IsTCP:   isTcp,
		}
		defer srv.queryLogger.Write(qle)
	}

	applog.Printf("[zone %s] incoming  %s %s (id %d) from %s\n", z.Origin, qnamefqdn,
		dnsutil.TypeToString(qtype), req.ID, w.RemoteAddr())

	applog.Println("Got request", req)

	// qlabel is the qname without the zone origin suffix. Pulling the
	// qname apart right here instead of in a helper keeps the
	// request setup in one place.
	qlabel := strings.TrimSuffix(qnamefqdn, ".")
	if qlabel != "" {
		lx := strings.Split(qlabel, ".")
		ql := lx[0 : len(lx)-z.LabelCount]
		qlabel = strings.ToLower(strings.Join(ql, "."))
	}

	z.Metrics.LabelStats.Add(qlabel)

	// IP that's talking to us (not EDNS CLIENT SUBNET)
	var realIP netip.Addr

	if addr, ok := w.RemoteAddr().(*net.UDPAddr); ok {
		realIP = addr.AddrPort().Addr()
	} else if addr, ok := w.RemoteAddr().(*net.TCPAddr); ok {
		realIP = addr.AddrPort().Addr()
	}
	if qle != nil {
		qle.RemoteAddr = realIP.String()
	}

	z.Metrics.ClientStats.Add(realIP.String())

	var ip netip.Addr // EDNS CLIENT SUBNET or real IP
	var ecs *dns.SUBNET

	// Scan the request's pseudo records for an OPT record here rather
	// than calling edns.FindOPT; the extra indirection doesn't pay
	// for itself on the hot path.
	var option *dns.OPT
	for _, rr := range req.Pseudo {
		if opt, ok := rr.(*dns.OPT); ok {
			option = opt
			break
		}
	}

	if option != nil {
		for _, s := range option.Options {
			switch e := s.(type) {
			case *dns.SUBNET:
				applog.Println("Got edns-client-subnet", e.Address, e.Family, e.Netmask, e.Scope)
				if e.Address.IsValid() {
					ecs = e

					ecsip := e.Address
					if ecsip.IsGlobalUnicast() &&
						!(ecsip.IsPrivate() ||
							ecsip.IsLinkLocalMulticast() ||
							ecsip.IsInterfaceLocalMulticast()) {
						ip = ecsip
					}

					if qle != nil {
						qle.HasECS = true
						qle.ClientAddr = fmt.Sprintf("%s/%d", ip, e.Netmask)
					}
				}
			}
		}
	}

	if !ip.IsValid() { // no edns client subnet
		ip = realIP
		if qle != nil {
			qle.ClientAddr = fmt.Sprintf("%s/%d", ip, len(ip.AsSlice())*8)
		}
	}

	// Get the targets for this query. The target computation is done
	// inline (it used to be two helper calls deep): this is the DNS
	// hot path, so we assemble the target list right here and only
	// fall back to the library call when we need to redo it for the
	// real IP.
	t := z.Options.Targeting
	targets := make([]string, 0)
	var netmask int
	var location *geo.Location

	if t&targeting.TargetIP > 0 {
		ipStr := ip.String()
		targets = append(targets, "["+ipStr+"]")
		if ip.Unmap().Is4() {
			masked := netip.PrefixFrom(ip, 24).Masked().Addr()
			if masked != ip {
				targets = append(targets, "["+masked.String()+"]")
			}
		} else {
			// v6 address, also target the /48 address
			ip48 := netip.PrefixFrom(ip, 48).Masked().Addr()
			targets = append(targets, "["+ip48.String()+"]")
		}
	}

	geoProvider := targeting.Geo()
	if geoProvider != nil {
		geotargets := make([]string, 0)

		if t&targeting.TargetASN > 0 {
			asn, _, err := geoProvider.GetASN(ip)
			if err != nil {
				log.Printf("GetASN error: %s", err)
			}
			if len(asn) > 0 {
				geotargets = append(geotargets, asn)
			}
		}

		var country, continent, region, regionGroup string

		geoOK := false

		if t&targeting.TargetRegion > 0 || t&targeting.TargetRegionGroup > 0 || z.HasClosest {
			var err error
			location, err = geoProvider.GetLocation(ip)
			if location == nil || err != nil {
				// the location lookup failed, so the best we
				// can offer is the ASN target (if any)
				location = nil
			} else {
				geoOK = true
				country = location.Country
				continent = location.Continent
				region = location.Region
				regionGroup = location.RegionGroup
			}
		} else if t&targeting.TargetCountry > 0 || t&targeting.TargetContinent > 0 {
			geoOK = true
			country, continent, netmask = geoProvider.GetCountry(ip)
		}

		if geoOK {
			if t&targeting.TargetRegion > 0 && len(region) > 0 {
				geotargets = append(geotargets, region)
			}
			if t&targeting.TargetRegionGroup > 0 && len(regionGroup) > 0 {
				geotargets = append(geotargets, regionGroup)
			}

			if t&targeting.TargetCountry > 0 && len(country) > 0 {
				geotargets = append(geotargets, country)
			}

			if t&targeting.TargetContinent > 0 && len(continent) > 0 {
				geotargets = append(geotargets, continent)
			}
		}

		targets = append(targets, geotargets...)
	}

	if t&targeting.TargetGlobal > 0 {
		targets = append(targets, "@")
	}

	// if the ECS IP didn't get targets, try the real IP instead
	if l := len(targets); (l == 0 || l == 1 && targets[0] == "@") && ip != realIP {
		targets, netmask, location = z.Options.Targeting.GetTargets(realIP, z.HasClosest)
	}

	m := &dns.Msg{}

	// setup logging of answers and rcode
	if qle != nil {
		qle.Targets = targets
		defer func() {
			qle.Rcode = int(m.Rcode)
			qle.AnswerCount = len(m.Answer)

			for _, rr := range m.Answer {
				var s string
				switch a := rr.(type) {
				case *dns.A:
					s = a.Addr.String()
				case *dns.AAAA:
					s = a.Addr.String()
				case *dns.CNAME:
					s = a.CNAME.Target
				case *dns.MX:
					s = a.MX.Mx
				case *dns.NS:
					s = a.NS.Ns
				case *dns.SRV:
					s = a.SRV.Target
				case *dns.TXT:
					s = strings.Join(a.TXT.Txt, " ")
				}
				if len(s) > 0 {
					qle.AnswerData = append(qle.AnswerData, s)
				}
			}
		}()
	}

	mv, err := edns.Version(req)
	if err != nil {
		m = mv
		if _, err := m.WriteTo(w); err != nil {
			applog.Printf("could not write response: %s", err)
		}
		return
	}

	dnsutil.SetReply(m, req)

	if option := edns.SetSizeAndDo(req, m); option != nil {
		for _, s := range option.Options {
			switch e := s.(type) {
			case *dns.NSID:
				e.Nsid = hex.EncodeToString([]byte(srv.info.ID))
			case *dns.SUBNET:
				// access e.Family, e.Address, etc.
				// TODO: set scope to 0 if there are no alternate responses
				if ecs != nil && ecs.Family != 0 {
					if netmask < 16 {
						netmask = 16
					}
					e.Scope = uint8(netmask)
				}
			}
		}
	}

	m.Authoritative = true

	// Find the labels for the question name, walking the targeting
	// chain inline (this used to be a helper method on the zone).
	qts := []uint16{dns.TypeMF, dns.TypeCNAME, qtype}

	labelMatches := make([]zones.LabelMatch, 0)

	for _, target := range targets {
		var name string

		switch target {
		case "@":
			name = qlabel
		default:
			if len(qlabel) > 0 {
				name = qlabel + "." + target
			} else {
				name = target
			}
		}

		if label, ok := z.Labels[name]; ok {
			var name string
			for _, qtype := range qts {
				switch qtype {
				case dns.TypeANY:
					// short-circuit mostly to avoid subtle bugs later
					// to be correct we should run through all the selectors and
					// pick types not already picked
					labelMatches = append(labelMatches, zones.LabelMatch{Label: z.Labels[qlabel], Type: qtype})
					continue
				case dns.TypeMF:
					if label.Records[dns.TypeMF] != nil {

						// don't follow NS and SOA records for aliases
						aliasQts := slices.DeleteFunc(qts, func(q uint16) bool {
							if slices.Contains(
								[]uint16{dns.TypeNS, dns.TypeSOA},
								q) {
								return true
							}
							return false
						})

						name = label.FirstRR(dns.TypeMF).(*dns.MF).Mf
						// TODO: need to avoid loops here somehow
						aliases := z.FindLabels(name, targets, aliasQts)
						labelMatches = append(labelMatches, aliases...)
						continue
					}
				default:
					// return the label if it has the right record
					if label.Records[qtype] != nil && len(label.Records[qtype]) > 0 {
						labelMatches = append(labelMatches, zones.LabelMatch{Label: label, Type: qtype})
						continue
					}
				}
			}
		}
	}

	if len(labelMatches) == 0 {
		// this is to make sure we return 'noerror' instead of 'nxdomain' when
		// appropriate.
		if label, ok := z.Labels[qlabel]; ok {
			labelMatches = append(labelMatches, zones.LabelMatch{Label: label})
		}
	}

	if len(labelMatches) == 0 {

		permitDebug := srv.PublicDebugQueries || (realIP.IsValid() && realIP.IsLoopback())

		firstLabel := (strings.Split(qlabel, "."))[0]

		if qle != nil {
			qle.LabelName = firstLabel
		}

		if permitDebug && firstLabel == "_status" {
			if qtype == dns.TypeANY || qtype == dns.TypeTXT {
				m.Answer = srv.statusRR(qlabel + "." + z.Origin + ".")
			} else {
				m.Ns = append(m.Ns, z.SoaRR())
			}
			m.Authoritative = true
			if _, err := m.WriteTo(w); err != nil {
				applog.Printf("error writing response: %s", err)
			}
			return
		}

		if permitDebug && firstLabel == "_health" {
			if qtype == dns.TypeANY || qtype == dns.TypeTXT {
				baseLabel := strings.Join((strings.Split(qlabel, "."))[1:], ".")
				m.Answer = z.HealthRR(qlabel+"."+z.Origin+".", baseLabel)
				m.Authoritative = true
				if _, err := m.WriteTo(w); err != nil {
					applog.Printf("error writing response: %s", err)
				}
				return
			}
			m.Ns = append(m.Ns, z.SoaRR())
			m.Authoritative = true
			if _, err := m.WriteTo(w); err != nil {
				applog.Printf("error writing response: %s", err)
			}
			return
		}

		if firstLabel == "_country" {
			if qtype == dns.TypeANY || qtype == dns.TypeTXT {
				h := dns.Header{TTL: 1, Class: dns.ClassINET}
				h.Name = qnamefqdn

				txt := []string{
					w.RemoteAddr().String(),
					ip.String(),
				}

				targets, netmask, location := z.Options.Targeting.GetTargets(ip, z.HasClosest)
				txt = append(txt, strings.Join(targets, " "))
				txt = append(txt, fmt.Sprintf("/%d", netmask), srv.info.ID, srv.info.IP)
				if location != nil {
					txt = append(txt, fmt.Sprintf("(%.3f,%.3f)", location.Latitude, location.Longitude))
				} else {
					txt = append(txt, "()")
				}

				m.Answer = []dns.RR{&dns.TXT{
					Hdr: h,
					TXT: rdata.TXT{Txt: txt},
				}}
			} else {
				m.Ns = append(m.Ns, z.SoaRR())
			}

			m.Authoritative = true

			if _, err := m.WriteTo(w); err != nil {
				applog.Printf("error writing response: %s", err)
			}
			return
		}

		// return NXDOMAIN
		m.Rcode = dns.RcodeNameError
		srv.metrics.Queries.With(
			prometheus.Labels{
				"zone":  z.Origin,
				"qtype": dnsutil.TypeToString(qtype),
				"qname": "_error",
				"rcode": dnsutil.RcodeToString(m.Rcode),
			}).Inc()
		m.Authoritative = true

		m.Ns = []dns.RR{z.SoaRR()}

		if _, err := m.WriteTo(w); err != nil {
			applog.Printf("error writing response: %s", err)
		}
		return
	}

	for _, match := range labelMatches {
		label := match.Label
		labelQtype := match.Type

		if !label.Closest {
			location = nil
		}

		// The picker is inlined: the records to answer with are
		// picked straight from the label here (including the health
		// filtering that used to be its own helper). The early
		// "returns" of the picking logic get out of the (one-pass)
		// loop below.
		var servers zones.Records

		for getPicked := true; getPicked; getPicked = false {

			if labelQtype == dns.TypeANY {
				var result zones.Records
				for rtype := range label.Records {

					rtypeRecords := z.Picker(label, rtype, label.MaxHosts, location)

					tmpResult := make(zones.Records, len(result)+len(rtypeRecords))

					copy(tmpResult, result)
					copy(tmpResult[len(result):], rtypeRecords)
					result = tmpResult
				}

				servers = result
				break
			}

			labelRR := label.Records[labelQtype]
			if labelRR == nil {
				// we don't have anything of the correct type
				break
			}

			sum := label.Weight[labelQtype]

			servers = make(zones.Records, len(labelRR))
			copy(servers, labelRR)

			if label.Test != nil {
				// Remove any unhealthy servers
				tmpServers := servers[:0]

				newSum := 0
				for i, s := range servers {
					if len(servers[i].Test) == 0 || health.GetStatus(servers[i].Test) == health.StatusHealthy {
						tmpServers = append(tmpServers, s)
						newSum += s.Weight
					}
				}
				servers = tmpServers
				sum = newSum
				// sum re-check to mirror the label.Weight[] check below
				if sum == 0 {
					// todo: this is wrong for cname since it misses
					// the 'max_hosts' setting
					break
				}
			}

			// not "balanced", just return all -- It's been working
			// this way since the first prototype, it might not make
			// sense anymore. This probably makes NS records and such
			// work as expected.
			// A, AAAA and CNAME records ("AlwaysWeighted") are always given
			// a weight so MaxHosts works for those even if weight isn't set.
			if label.Weight[labelQtype] == 0 {
				break
			}

			max := label.MaxHosts
			if labelQtype == dns.TypeCNAME || labelQtype == dns.TypeMF {
				max = 1
			}

			rrCount := len(servers)
			if max > rrCount {
				max = rrCount
			}
			result := make(zones.Records, max)

			// Find the distance to each server, and find the servers that are
			// closer to the querier than the max'th furthest server, or within
			// 5% thereof. What this means in practice is that if we have a nearby
			// cluster of servers that are close, they all get included, so load
			// balancing works
			if location != nil && (labelQtype == dns.TypeA || labelQtype == dns.TypeAAAA) && max < rrCount {
				// First we record the distance to each server
				distances := make([]float64, rrCount)
				for i, s := range servers {
					distance := location.Distance(s.Loc)
					distances[i] = distance
				}

				// though this looks like O(n^2), typically max is small (e.g. 2)
				// servers often have the same geographic location
				// and rrCount is pretty small too, so the gain of an
				// O(n log n) sort is small.
				chosen := 0
				choose := make([]bool, rrCount)

				for chosen < max {
					// Determine the minimum distance of servers not yet chosen
					minDist := location.MaxDistance()
					for i := range servers {
						if !choose[i] && distances[i] <= minDist {
							minDist = distances[i]
						}
					}
					// The threshold for inclusion on the this pass is 5% more
					// than the minimum distance
					minDist = minDist * 1.05
					// Choose all the servers within the distance
					for i := range servers {
						if !choose[i] && distances[i] <= minDist {
							choose[i] = true
							chosen++
						}
					}
				}

				// Now choose only the chosen servers, using filtering without allocation
				// slice trick. Meanwhile recalculate the total weight
				tmpServers := servers[:0]
				sum = 0
				for i, s := range servers {
					if choose[i] {
						tmpServers = append(tmpServers, s)
						sum += s.Weight
					}
				}
				servers = tmpServers
			}

			for si := 0; si < max; si++ {
				n := rand.Intn(sum + 1)
				s := 0

				for i := range servers {
					s += int(servers[i].Weight)
					if s >= n {
						sum -= servers[i].Weight
						result[si] = servers[i]

						// remove the server from the list
						servers = append(servers[:i], servers[i+1:]...)
						break
					}
				}
			}

			servers = result

			break
		}

		if servers != nil {
			var rrs []dns.RR
			for _, record := range servers {
				rr := record.RR.Clone()
				rr.Header().Name = qnamefqdn
				rrs = append(rrs, rr)
			}
			m.Answer = rrs
		}
		if len(m.Answer) > 0 {
			// maxHosts only matter within a "targeting group"; at least that's
			// how it has been working, so we stop looking for answers as soon
			// we have some.

			if qle != nil {
				qle.LabelName = label.Label
				qle.AnswerCount = len(m.Answer)
			}

			break
		}
	}

	if len(m.Answer) == 0 {
		// Return a SOA so the NOERROR answer gets cached
		m.Ns = append(m.Ns, z.SoaRR())
	}

	qlabelMetric := "_"
	if srv.DetailedMetrics {
		qlabelMetric = qlabel
	}

	srv.metrics.Queries.With(
		prometheus.Labels{
			"zone":  z.Origin,
			"qtype": dnsutil.TypeToString(qtype),
			"qname": qlabelMetric,
			"rcode": dnsutil.RcodeToString(m.Rcode),
		}).Inc()

	applog.Println(m)

	if qle != nil {
		// should this be in the match loop above?
		qle.Rcode = int(m.Rcode)
	}
	if _, err = m.WriteTo(w); err != nil {
		// if Pack'ing fails the Write fails. Return SERVFAIL.
		applog.Printf("Error writing packet: %q, %s", err, m)
		// Handle failed manually - create SERVFAIL response
		sf := new(dns.Msg)
		dnsutil.SetReply(sf, req)
		sf.Rcode = dns.RcodeServerFailure
		if _, err := sf.WriteTo(w); err != nil {
			applog.Printf("error writing SERVFAIL response: %s", err)
		}
	}
}

func (srv *Server) statusRR(label string) []dns.RR {
	h := dns.Header{TTL: 1, Class: dns.ClassINET}
	h.Name = label

	status := map[string]string{"v": srv.info.Version, "id": srv.info.ID}

	hostname, err := os.Hostname()
	if err == nil {
		status["h"] = hostname
	}

	status["up"] = strconv.Itoa(int(time.Since(srv.info.Started).Seconds()))

	js, err := json.Marshal(status)
	if err != nil {
		log.Printf("error marshaling json status: %s", err)
	}

	return []dns.RR{&dns.TXT{Hdr: h, TXT: rdata.TXT{Txt: []string{string(js)}}}}
}
