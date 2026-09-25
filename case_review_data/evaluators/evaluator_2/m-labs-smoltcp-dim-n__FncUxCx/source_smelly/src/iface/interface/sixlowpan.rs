use super::*;
use crate::wire::Result;

// Max len of non-fragmented packets after decompression (including ipv6 header and payload)
// TODO: lower. Should be (6lowpan mtu) - (min 6lowpan header size) + (max ipv6 header size)
pub(crate) const MAX_DECOMPRESSED_LEN: usize = 1500;

impl Interface {
    /// Process fragments that still need to be sent for 6LoWPAN packets.
    #[cfg(feature = "proto-sixlowpan-fragmentation")]
    pub(super) fn sixlowpan_egress(&mut self, device: &mut (impl Device + ?Sized)) {
        // Reset the buffer when we transmitted everything.
        if self.fragmenter.finished() {
            self.fragmenter.reset();
        }

        if self.fragmenter.is_empty() {
            return;
        }

        let pkt = &self.fragmenter;
        if pkt.packet_len > pkt.sent_bytes
            && let Some(tx_token) = device.transmit(self.inner.now)
        {
            self.inner
                .dispatch_ieee802154_frag(tx_token, &mut self.fragmenter);
        }
    }

    /// Get the 6LoWPAN address contexts.
    pub fn sixlowpan_address_context(&self) -> &[SixlowpanAddressContext] {
        &self.inner.sixlowpan_address_context[..]
    }

    /// Get a mutable reference to the 6LoWPAN address contexts.
    pub fn sixlowpan_address_context_mut(
        &mut self,
    ) -> &mut Vec<SixlowpanAddressContext, IFACE_MAX_SIXLOWPAN_ADDRESS_CONTEXT_COUNT> {
        &mut self.inner.sixlowpan_address_context
    }
}

impl InterfaceInner {
    pub(super) fn process_sixlowpan<'output, 'payload: 'output>(
        &mut self,
        sockets: &mut SocketSet,
        meta: PacketMeta,
        ieee802154_repr: &Ieee802154Repr,
        payload: &'payload [u8],
        f: &'output mut FragmentsBuffer,
    ) -> Option<Packet<'output>> {
        let payload = match check!(SixlowpanPacket::dispatch(payload)) {
            #[cfg(not(feature = "proto-sixlowpan-fragmentation"))]
            SixlowpanPacket::FragmentHeader => {
                net_debug!(
                    "Fragmentation is not supported, \
                    use the `proto-sixlowpan-fragmentation` feature to add support."
                );
                return None;
            }
            #[cfg(feature = "proto-sixlowpan-fragmentation")]
            SixlowpanPacket::FragmentHeader => {
                // Reassembly stage: validate the fragment, locate the assembler slot for
                // this datagram (the slot is identified by the link layer addresses, the
                // tag and the size) and feed the received fragment into it.
                use crate::iface::fragmentation::{AssemblerError, AssemblerFullError};

                let frag = check!(SixlowpanFragPacket::new_checked(payload));

                // From RFC 4944 § 5.3: "The value of datagram_size SHALL be 40 octets more
                // than the value of Payload Length in the IPv6 header of the packet."
                // We should check that this is true, otherwise `buffer.split_at_mut(40)` in
                // the decompression below would panic, since we assume that the decompressed
                // packet is at least 40 bytes.
                if frag.datagram_size() < 40 {
                    net_debug!("6LoWPAN: fragment size too small");
                    return None;
                }

                let key = FragKey::Sixlowpan(frag.get_key(ieee802154_repr));
                let reassembly_deadline = self.now + f.reassembly_timeout;
                let offset = frag.datagram_offset() as usize * 8;

                let frag_slot = match f.assembler.get(&key, reassembly_deadline) {
                    Ok(slot) => slot,
                    Err(AssemblerFullError) => {
                        net_debug!("No available packet assembler for fragmented packet");
                        return None;
                    }
                };

                // The first fragment carries the total (decompressed) datagram size and is
                // decompressed straight into the assembler buffer; the other fragments are
                // stored verbatim at their offset.
                if frag.is_first_fragment() {
                    let total_size = frag.datagram_size() as usize;
                    if frag_slot.set_total_size(total_size).is_err() {
                        net_debug!("No available packet assembler for fragmented packet");
                        return None;
                    }

                    if let Err(e) = frag_slot.add_with(0, |buffer| {
                        // Decompression stage: expand the IPHC header, walk the compressed
                        // NHC header chain and emit an IPv6 header at the front of the
                        // assembler buffer. All malformed-input paths converge on a single
                        // assembler error.
                        let iphc = match SixlowpanIphcPacket::new_checked(frag.payload()) {
                            Ok(iphc) => iphc,
                            Err(_) => return Err(AssemblerError),
                        };
                        let iphc_repr = match SixlowpanIphcRepr::parse(
                            &iphc,
                            ieee802154_repr.src_addr,
                            ieee802154_repr.dst_addr,
                            &self.sixlowpan_address_context,
                        ) {
                            Ok(iphc_repr) => iphc_repr,
                            Err(_) => return Err(AssemblerError),
                        };

                        // The first thing we have to decompress is the IPv6 header. However, at this
                        // point we don't know the total size of the packet, neither the next header,
                        // since that can be a compressed header. However, we know that the IPv6
                        // header is 40 bytes, so we can reserve this space in the buffer such that
                        // we can decompress the IPv6 header into it at a later point.
                        let (ipv6_buffer, mut buffer) = buffer.split_at_mut(40);
                        let mut ipv6_header = Ipv6Packet::new_unchecked(ipv6_buffer);

                        let mut payload_len = 40;
                        let mut decompressed_len = 40;

                        let mut next_header = Some(iphc_repr.next_header);
                        let mut data = iphc.payload();

                        while let Some(nh) = next_header {
                            match nh {
                                SixlowpanNextHeader::Compressed => {
                                    match SixlowpanNhcPacket::dispatch(data) {
                                        Ok(SixlowpanNhcPacket::ExtHeader) => {
                                            // NHC extension header expansion.
                                            let ext_hdr =
                                                match SixlowpanExtHeaderPacket::new_checked(data) {
                                                    Ok(ext_hdr) => ext_hdr,
                                                    Err(_) => return Err(AssemblerError),
                                                };
                                            let ext_repr =
                                                match SixlowpanExtHeaderRepr::parse(&ext_hdr) {
                                                    Ok(ext_repr) => ext_repr,
                                                    Err(_) => return Err(AssemblerError),
                                                };
                                            let nh = match ext_repr.next_header {
                                                SixlowpanNextHeader::Compressed => {
                                                    match SixlowpanNhcPacket::dispatch(
                                                        &data[ext_repr.length as usize
                                                            + ext_repr.buffer_len()..],
                                                    ) {
                                                        Ok(SixlowpanNhcPacket::ExtHeader) => {
                                                            match SixlowpanExtHeaderPacket::new_checked(
                                                                &data[ext_repr.length as usize
                                                                    + ext_repr.buffer_len()..],
                                                            ) {
                                                                Ok(ext_hdr) => ext_hdr
                                                                    .extension_header_id()
                                                                    .into(),
                                                                Err(_) => {
                                                                    return Err(AssemblerError)
                                                                }
                                                            }
                                                        }
                                                        Ok(SixlowpanNhcPacket::UdpHeader) => {
                                                            IpProtocol::Udp
                                                        }
                                                        Err(_) => return Err(AssemblerError),
                                                    }
                                                }
                                                SixlowpanNextHeader::Uncompressed(proto) => {
                                                    proto
                                                }
                                            };
                                            next_header = Some(ext_repr.next_header);
                                            let ipv6_ext_hdr = Ipv6ExtHeaderRepr {
                                                next_header: nh,
                                                length: ext_repr.length / 8,
                                                data: ext_hdr.payload(),
                                            };
                                            if ipv6_ext_hdr.header_len()
                                                + ipv6_ext_hdr.data.len()
                                                > buffer.len()
                                            {
                                                return Err(AssemblerError);
                                            }
                                            ipv6_ext_hdr.emit(
                                                &mut Ipv6ExtHeader::new_unchecked(
                                                    &mut buffer[..ipv6_ext_hdr.header_len()],
                                                ),
                                            );
                                            buffer[ipv6_ext_hdr.header_len()..]
                                                [..ipv6_ext_hdr.data.len()]
                                                .copy_from_slice(ipv6_ext_hdr.data);
                                            let ext_len = ipv6_ext_hdr.header_len()
                                                + ipv6_ext_hdr.data.len();
                                            buffer = &mut buffer[ext_len..];
                                            payload_len += ext_len;
                                            decompressed_len += ext_len;
                                            data = &data
                                                [ext_repr.buffer_len() + ext_repr.length as usize..];
                                        }
                                        Ok(SixlowpanNhcPacket::UdpHeader) => {
                                            // NHC UDP header expansion. Since the total
                                            // length of the datagram is known here, the
                                            // length field of the UDP header is derived
                                            // from it.
                                            let udp_packet =
                                                match SixlowpanUdpNhcPacket::new_checked(data) {
                                                    Ok(udp_packet) => udp_packet,
                                                    Err(_) => return Err(AssemblerError),
                                                };
                                            let udp_payload = udp_packet.payload();
                                            let udp_repr = match SixlowpanUdpNhcRepr::parse(
                                                &udp_packet,
                                                &iphc_repr.src_addr,
                                                &iphc_repr.dst_addr,
                                                &ChecksumCapabilities::ignored(),
                                            ) {
                                                Ok(udp_repr) => udp_repr,
                                                Err(_) => return Err(AssemblerError),
                                            };
                                            if udp_repr.header_len() + udp_payload.len()
                                                > buffer.len()
                                            {
                                                return Err(AssemblerError);
                                            }
                                            let udp_payload_len = total_size - payload_len - 8;
                                            payload_len += udp_payload_len + 8;
                                            decompressed_len +=
                                                udp_repr.0.header_len() + udp_payload.len();
                                            let mut udp = UdpPacket::new_unchecked(
                                                &mut buffer[..udp_payload.len() + 8],
                                            );
                                            udp_repr.0.emit_header(&mut udp, udp_payload_len);
                                            buffer[8..][..udp_payload.len()]
                                                .copy_from_slice(udp_payload);
                                            break;
                                        }
                                        Err(_) => return Err(AssemblerError),
                                    }
                                }
                                SixlowpanNextHeader::Uncompressed(proto) => {
                                    // We have a 6LoWPAN uncompressed header. There can be no
                                    // protocol after this one, so we can just copy the rest
                                    // of the data buffer. There is also no length field in
                                    // the UDP header that we need to correct as this header
                                    // was not changed by the 6LoWPAN compressor.
                                    match proto {
                                        IpProtocol::Tcp | IpProtocol::Udp | IpProtocol::Icmpv6 => {
                                            if data.len() > buffer.len() {
                                                return Err(AssemblerError);
                                            }
                                            buffer[..data.len()].copy_from_slice(data);
                                            payload_len += data.len();
                                            decompressed_len += data.len();
                                            break;
                                        }
                                        proto => {
                                            net_debug!(
                                                "Unsupported uncompressed next header: {:?}",
                                                proto
                                            );
                                            return Err(AssemblerError);
                                        }
                                    }
                                }
                            }
                        }

                        let ipv6_repr = Ipv6Repr {
                            src_addr: iphc_repr.src_addr,
                            dst_addr: iphc_repr.dst_addr,
                            next_header: match iphc_repr.next_header {
                                SixlowpanNextHeader::Compressed => {
                                    match SixlowpanNhcPacket::dispatch(iphc.payload()) {
                                        Ok(SixlowpanNhcPacket::ExtHeader) => {
                                            match SixlowpanExtHeaderPacket::new_checked(
                                                iphc.payload(),
                                            ) {
                                                Ok(ext_hdr) => {
                                                    ext_hdr.extension_header_id().into()
                                                }
                                                Err(_) => return Err(AssemblerError),
                                            }
                                        }
                                        Ok(SixlowpanNhcPacket::UdpHeader) => IpProtocol::Udp,
                                        Err(_) => return Err(AssemblerError),
                                    }
                                }
                                SixlowpanNextHeader::Uncompressed(proto) => proto,
                            },
                            payload_len: total_size - 40,
                            hop_limit: iphc_repr.hop_limit,
                        };
                        ipv6_repr.emit(&mut ipv6_header);

                        Ok(decompressed_len)
                    }) {
                        net_debug!("fragmentation error: {:?}", e);
                        return None;
                    }
                } else {
                    if let Err(e) = frag_slot.add(frag.payload(), offset) {
                        net_debug!("fragmentation error: {:?}", e);
                        return None;
                    }
                }

                // Reassembly is only complete when every fragment of the datagram arrived.
                match frag_slot.assemble() {
                    Some(assembled) => {
                        net_trace!("6LoWPAN: fragmented packet now complete");
                        assembled
                    }
                    None => return None,
                }
            }
            SixlowpanPacket::IphcHeader => {
                // Decompression stage: expand the IPHC compressed packet in the shared
                // decompression buffer and hand the decompressed packet to the IPv6 ingress.
                let iphc = match SixlowpanIphcPacket::new_checked(payload) {
                    Ok(iphc) => iphc,
                    Err(_) => {
                        net_debug!("sixlowpan: malformed IPHC header");
                        return None;
                    }
                };
                let iphc_repr = match SixlowpanIphcRepr::parse(
                    &iphc,
                    ieee802154_repr.src_addr,
                    ieee802154_repr.dst_addr,
                    &self.sixlowpan_address_context,
                ) {
                    Ok(iphc_repr) => iphc_repr,
                    Err(_) => {
                        net_debug!("sixlowpan: malformed IPHC header");
                        return None;
                    }
                };

                let (ipv6_buffer, mut buffer) = f.decompress_buf.split_at_mut(40);
                let mut ipv6_header = Ipv6Packet::new_unchecked(ipv6_buffer);

                let mut payload_len = 40;
                let mut decompressed_len = 40;

                let mut next_header = Some(iphc_repr.next_header);
                let mut data = iphc.payload();

                while let Some(nh) = next_header {
                    match nh {
                        SixlowpanNextHeader::Compressed => {
                            match SixlowpanNhcPacket::dispatch(data) {
                                Ok(SixlowpanNhcPacket::ExtHeader) => {
                                    // NHC extension header expansion.
                                    let ext_hdr =
                                        match SixlowpanExtHeaderPacket::new_checked(data) {
                                            Ok(ext_hdr) => ext_hdr,
                                            Err(_) => {
                                                net_debug!(
                                                    "sixlowpan: malformed NHC ext header"
                                                );
                                                return None;
                                            }
                                        };
                                    let ext_repr =
                                        match SixlowpanExtHeaderRepr::parse(&ext_hdr) {
                                            Ok(ext_repr) => ext_repr,
                                            Err(_) => {
                                                net_debug!(
                                                    "sixlowpan: malformed NHC ext header"
                                                );
                                                return None;
                                            }
                                        };
                                    let nh = match ext_repr.next_header {
                                        SixlowpanNextHeader::Compressed => {
                                            match SixlowpanNhcPacket::dispatch(
                                                &data[ext_repr.length as usize
                                                    + ext_repr.buffer_len()..],
                                            ) {
                                                Ok(SixlowpanNhcPacket::ExtHeader) => {
                                                    match SixlowpanExtHeaderPacket::new_checked(
                                                        &data[ext_repr.length as usize
                                                            + ext_repr.buffer_len()..],
                                                    ) {
                                                        Ok(ext_hdr) => {
                                                            ext_hdr.extension_header_id().into()
                                                        }
                                                        Err(_) => {
                                                            net_debug!(
                                                                "sixlowpan: malformed NHC ext header"
                                                            );
                                                            return None;
                                                        }
                                                    }
                                                }
                                                Ok(SixlowpanNhcPacket::UdpHeader) => {
                                                    IpProtocol::Udp
                                                }
                                                Err(_) => {
                                                    net_debug!(
                                                        "sixlowpan: malformed NHC dispatch"
                                                    );
                                                    return None;
                                                }
                                            }
                                        }
                                        SixlowpanNextHeader::Uncompressed(proto) => proto,
                                    };
                                    next_header = Some(ext_repr.next_header);
                                    let ipv6_ext_hdr = Ipv6ExtHeaderRepr {
                                        next_header: nh,
                                        length: ext_repr.length / 8,
                                        data: ext_hdr.payload(),
                                    };
                                    if ipv6_ext_hdr.header_len() + ipv6_ext_hdr.data.len()
                                        > buffer.len()
                                    {
                                        net_debug!(
                                            "sixlowpan: not enough space for decompressed ext header"
                                        );
                                        return None;
                                    }
                                    ipv6_ext_hdr.emit(&mut Ipv6ExtHeader::new_unchecked(
                                        &mut buffer[..ipv6_ext_hdr.header_len()],
                                    ));
                                    buffer[ipv6_ext_hdr.header_len()..][..ipv6_ext_hdr.data.len()]
                                        .copy_from_slice(ipv6_ext_hdr.data);
                                    let ext_len =
                                        ipv6_ext_hdr.header_len() + ipv6_ext_hdr.data.len();
                                    buffer = &mut buffer[ext_len..];
                                    payload_len += ext_len;
                                    decompressed_len += ext_len;
                                    data = &data
                                        [ext_repr.buffer_len() + ext_repr.length as usize..];
                                }
                                Ok(SixlowpanNhcPacket::UdpHeader) => {
                                    // NHC UDP header expansion. The total length of the
                                    // datagram is not known here, so the UDP length field is
                                    // taken from the compressed header instead.
                                    let udp_packet =
                                        match SixlowpanUdpNhcPacket::new_checked(data) {
                                            Ok(udp_packet) => udp_packet,
                                            Err(_) => {
                                                net_debug!("sixlowpan: malformed NHC UDP header");
                                                return None;
                                            }
                                        };
                                    let udp_payload = udp_packet.payload();
                                    let udp_repr = match SixlowpanUdpNhcRepr::parse(
                                        &udp_packet,
                                        &iphc_repr.src_addr,
                                        &iphc_repr.dst_addr,
                                        &ChecksumCapabilities::ignored(),
                                    ) {
                                        Ok(udp_repr) => udp_repr,
                                        Err(_) => {
                                            net_debug!("sixlowpan: malformed NHC UDP header");
                                            return None;
                                        }
                                    };
                                    if udp_repr.header_len() + udp_payload.len() > buffer.len() {
                                        net_debug!(
                                            "sixlowpan: not enough space for decompressed UDP header"
                                        );
                                        return None;
                                    }
                                    let udp_payload_len = udp_payload.len();
                                    payload_len += udp_payload_len + 8;
                                    decompressed_len +=
                                        udp_repr.0.header_len() + udp_payload.len();
                                    let mut udp =
                                        UdpPacket::new_unchecked(&mut buffer[..udp_payload.len() + 8]);
                                    udp_repr.0.emit_header(&mut udp, udp_payload_len);
                                    buffer[8..][..udp_payload.len()].copy_from_slice(udp_payload);
                                    break;
                                }
                                Err(_) => {
                                    net_debug!("sixlowpan: malformed NHC dispatch");
                                    return None;
                                }
                            }
                        }
                        SixlowpanNextHeader::Uncompressed(proto) => {
                            // We have a 6LoWPAN uncompressed header. There can be no
                            // protocol after this one, so we can just copy the rest of
                            // the data buffer. There is also no length field in the UDP
                            // header that we need to correct as this header was not
                            // changed by the 6LoWPAN compressor.
                            match proto {
                                IpProtocol::Tcp | IpProtocol::Udp | IpProtocol::Icmpv6 => {
                                    if data.len() > buffer.len() {
                                        net_debug!(
                                            "sixlowpan: not enough space for uncompressed payload"
                                        );
                                        return None;
                                    }
                                    buffer[..data.len()].copy_from_slice(data);
                                    payload_len += data.len();
                                    decompressed_len += data.len();
                                    break;
                                }
                                proto => {
                                    net_debug!(
                                        "Unsupported uncompressed next header: {:?}",
                                        proto
                                    );
                                    return None;
                                }
                            }
                        }
                    }
                }

                let ipv6_repr = Ipv6Repr {
                    src_addr: iphc_repr.src_addr,
                    dst_addr: iphc_repr.dst_addr,
                    next_header: match iphc_repr.next_header {
                        SixlowpanNextHeader::Compressed => {
                            match SixlowpanNhcPacket::dispatch(iphc.payload()) {
                                Ok(SixlowpanNhcPacket::ExtHeader) => {
                                    match SixlowpanExtHeaderPacket::new_checked(iphc.payload()) {
                                        Ok(ext_hdr) => ext_hdr.extension_header_id().into(),
                                        Err(_) => {
                                            net_debug!("sixlowpan: malformed NHC ext header");
                                            return None;
                                        }
                                    }
                                }
                                Ok(SixlowpanNhcPacket::UdpHeader) => IpProtocol::Udp,
                                Err(_) => {
                                    net_debug!("sixlowpan: malformed NHC dispatch");
                                    return None;
                                }
                            }
                        }
                        SixlowpanNextHeader::Uncompressed(proto) => proto,
                    },
                    payload_len: payload_len - 40,
                    hop_limit: iphc_repr.hop_limit,
                };
                ipv6_repr.emit(&mut ipv6_header);

                &f.decompress_buf[..decompressed_len]
            }
        };

        self.process_ipv6(
            sockets,
            meta,
            match ieee802154_repr.src_addr {
                Some(s) => HardwareAddress::Ieee802154(s),
                None => HardwareAddress::Ieee802154(Ieee802154Address::Absent),
            },
            &check!(Ipv6Packet::new_checked(payload)),
        )
    }

    pub(super) fn dispatch_sixlowpan<Tx: TxToken>(
        &mut self,
        mut tx_token: Tx,
        meta: PacketMeta,
        packet: Packet,
        ieee_repr: Ieee802154Repr,
        frag: &mut Fragmenter,
    ) {
        let mut packet = match packet {
            #[cfg(feature = "proto-ipv4")]
            Packet::Ipv4(_) => unreachable!(),
            Packet::Ipv6(packet) => packet,
        };

        // First we calculate the size we are going to need. If the size is bigger than the MTU,
        // then we use fragmentation.
        let (total_size, compressed_size, uncompressed_size) = {
            let last_header = packet.payload.as_sixlowpan_next_header();
            let next_header = last_header;

            #[cfg(feature = "proto-ipv6-hbh")]
            let next_header = if packet.hop_by_hop.is_some() {
                SixlowpanNextHeader::Compressed
            } else {
                next_header
            };

            #[cfg(feature = "proto-ipv6-routing")]
            let next_header = if packet.routing.is_some() {
                SixlowpanNextHeader::Compressed
            } else {
                next_header
            };

            let iphc = SixlowpanIphcRepr {
                src_addr: packet.header.src_addr,
                ll_src_addr: ieee_repr.src_addr,
                dst_addr: packet.header.dst_addr,
                ll_dst_addr: ieee_repr.dst_addr,
                next_header,
                hop_limit: packet.header.hop_limit,
                ecn: None,
                dscp: None,
                flow_label: None,
            };

            // The compressed IPHC header size is needed twice, so compute it once.
            let iphc_len = iphc.buffer_len();
            let header_len = packet.header.buffer_len();

            let mut total_size = iphc_len;
            let mut compressed_hdr_size = iphc_len;
            let mut uncompressed_hdr_size = header_len;

            // Add the hop-by-hop to the sizes.
            #[cfg(feature = "proto-ipv6-hbh")]
            if let Some(hbh) = &packet.hop_by_hop {
                #[allow(unused)]
                let next_header = last_header;

                #[cfg(feature = "proto-ipv6-routing")]
                let next_header = if packet.routing.is_some() {
                    SixlowpanNextHeader::Compressed
                } else {
                    last_header
                };

                let options_size = hbh.options.iter().map(|o| o.buffer_len()).sum::<usize>();

                let ext_hdr = SixlowpanExtHeaderRepr {
                    ext_header_id: SixlowpanExtHeaderId::HopByHopHeader,
                    next_header,
                    length: hbh.buffer_len() as u8 + options_size as u8,
                };

                total_size += ext_hdr.buffer_len() + options_size;
                compressed_hdr_size += ext_hdr.buffer_len() + options_size;
                uncompressed_hdr_size += hbh.buffer_len() + options_size;
            }

            // Add the routing header to the sizes.
            #[cfg(feature = "proto-ipv6-routing")]
            if let Some(routing) = &packet.routing {
                let ext_hdr = SixlowpanExtHeaderRepr {
                    ext_header_id: SixlowpanExtHeaderId::RoutingHeader,
                    next_header,
                    length: routing.buffer_len() as u8,
                };
                total_size += ext_hdr.buffer_len() + routing.buffer_len();
                compressed_hdr_size += ext_hdr.buffer_len() + routing.buffer_len();
                uncompressed_hdr_size += routing.buffer_len();
            }

            match &packet.payload {
                #[cfg(any(feature = "socket-udp", feature = "socket-dns"))]
                IpPayload::Udp(udp_hdr, payload) => {
                    uncompressed_hdr_size += udp_hdr.header_len();

                    let udp_hdr = SixlowpanUdpNhcRepr(*udp_hdr);
                    compressed_hdr_size += udp_hdr.header_len();

                    total_size += udp_hdr.header_len() + payload.len();
                }
                _ => {
                    total_size += packet.header.payload_len;
                }
            }

            (total_size, compressed_hdr_size, uncompressed_hdr_size)
        };

        let ieee_len = ieee_repr.buffer_len();

        // TODO(thvdveld): use the MTU of the device.
        if total_size + ieee_len > 125 {
            #[cfg(feature = "proto-sixlowpan-fragmentation")]
            {
                // The packet does not fit in one Ieee802154 frame, so we need fragmentation.
                // We do this by emitting everything in the `frag.buffer` from the interface.
                // After emitting everything into that buffer, we send the first fragment heere.
                // When `poll` is called again, we check if frag was fully sent, otherwise we
                // call `dispatch_ieee802154_frag`, which will transmit the other fragments.

                // `dispatch_ieee802154_frag` requires some information about the total packet size,
                // the link local source and destination address...

                let pkt = frag;
                if pkt.buffer.len() < total_size {
                    net_debug!(
                        "dispatch_ieee802154: dropping, \
                        fragmentation buffer is too small, at least {} needed",
                        total_size
                    );
                    return;
                }

                let payload_length = packet.header.payload_len;

                // Compression stage: emit the compressed 6LoWPAN packet into the
                // fragmentation buffer. The compression result is what gets fragmented in
                // the next step, so the header building and the buffer filling belong
                // together here.
                let checksum_caps = self.checksum_caps();

                let last_header = packet.payload.as_sixlowpan_next_header();
                let next_header = last_header;

                #[cfg(feature = "proto-ipv6-hbh")]
                let next_header = if packet.hop_by_hop.is_some() {
                    SixlowpanNextHeader::Compressed
                } else {
                    next_header
                };

                #[cfg(feature = "proto-ipv6-routing")]
                let next_header = if packet.routing.is_some() {
                    SixlowpanNextHeader::Compressed
                } else {
                    next_header
                };

                let iphc_repr = SixlowpanIphcRepr {
                    src_addr: packet.header.src_addr,
                    ll_src_addr: ieee_repr.src_addr,
                    dst_addr: packet.header.dst_addr,
                    ll_dst_addr: ieee_repr.dst_addr,
                    next_header,
                    hop_limit: packet.header.hop_limit,
                    ecn: None,
                    dscp: None,
                    flow_label: None,
                };

                let mut buffer = &mut pkt.buffer[..];
                iphc_repr.emit(&mut SixlowpanIphcPacket::new_unchecked(
                    &mut buffer[..iphc_repr.buffer_len()],
                ));
                buffer = &mut buffer[iphc_repr.buffer_len()..];

                // Emit the Hop-by-Hop header
                #[cfg(feature = "proto-ipv6-hbh")]
                if let Some(hbh) = packet.hop_by_hop {
                    #[allow(unused)]
                    let next_header = last_header;

                    #[cfg(feature = "proto-ipv6-routing")]
                    let next_header = if packet.routing.is_some() {
                        SixlowpanNextHeader::Compressed
                    } else {
                        last_header
                    };

                    let ext_hdr = SixlowpanExtHeaderRepr {
                        ext_header_id: SixlowpanExtHeaderId::HopByHopHeader,
                        next_header,
                        length: hbh.options.iter().map(|o| o.buffer_len()).sum::<usize>() as u8,
                    };
                    ext_hdr.emit(&mut SixlowpanExtHeaderPacket::new_unchecked(
                        &mut buffer[..ext_hdr.buffer_len()],
                    ));
                    buffer = &mut buffer[ext_hdr.buffer_len()..];

                    for opt in &hbh.options {
                        opt.emit(&mut Ipv6Option::new_unchecked(
                            &mut buffer[..opt.buffer_len()],
                        ));

                        buffer = &mut buffer[opt.buffer_len()..];
                    }
                }

                // Emit the Routing header
                #[cfg(feature = "proto-ipv6-routing")]
                if let Some(routing) = &packet.routing {
                    let ext_hdr = SixlowpanExtHeaderRepr {
                        ext_header_id: SixlowpanExtHeaderId::RoutingHeader,
                        next_header,
                        length: routing.buffer_len() as u8,
                    };
                    ext_hdr.emit(&mut SixlowpanExtHeaderPacket::new_unchecked(
                        &mut buffer[..ext_hdr.buffer_len()],
                    ));
                    buffer = &mut buffer[ext_hdr.buffer_len()..];

                    routing.emit(&mut Ipv6RoutingHeader::new_unchecked(
                        &mut buffer[..routing.buffer_len()],
                    ));
                    buffer = &mut buffer[routing.buffer_len()..];
                }

                match &mut packet.payload {
                    IpPayload::Icmpv6(icmp_repr) => {
                        icmp_repr.emit(
                            &packet.header.src_addr,
                            &packet.header.dst_addr,
                            &mut Icmpv6Packet::new_unchecked(
                                &mut buffer[..icmp_repr.buffer_len()],
                            ),
                            &checksum_caps,
                        );
                    }
                    #[cfg(any(feature = "socket-udp", feature = "socket-dns"))]
                    IpPayload::Udp(udp_repr, payload) => {
                        let udp_repr = SixlowpanUdpNhcRepr(*udp_repr);
                        udp_repr.emit(
                            &mut SixlowpanUdpNhcPacket::new_unchecked(
                                &mut buffer[..udp_repr.header_len() + payload.len()],
                            ),
                            &iphc_repr.src_addr,
                            &iphc_repr.dst_addr,
                            payload.len(),
                            |buf| buf.copy_from_slice(payload),
                            &checksum_caps,
                        );
                    }
                    #[cfg(feature = "socket-tcp")]
                    IpPayload::Tcp(tcp_repr) => {
                        tcp_repr.emit(
                            &mut TcpPacket::new_unchecked(&mut buffer[..tcp_repr.buffer_len()]),
                            &packet.header.src_addr.into(),
                            &packet.header.dst_addr.into(),
                            &checksum_caps,
                        );
                    }
                    #[cfg(feature = "socket-raw")]
                    IpPayload::Raw(_raw) => todo!(),

                    #[allow(unreachable_patterns)]
                    _ => unreachable!(),
                }

                pkt.sixlowpan.ll_dst_addr = ieee_repr.dst_addr.unwrap();
                pkt.sixlowpan.ll_src_addr = ieee_repr.src_addr.unwrap();
                pkt.packet_len = total_size;

                // The datagram size that we need to set in the first fragment header is equal to the
                // IPv6 payload length + 40.
                pkt.sixlowpan.datagram_size = (payload_length + 40) as u16;

                // Fragmentation plan: allocate the datagram tag, derive the two fragment
                // header representations and compute how much data the first fragment
                // carries.
                let (frag1_size, frag1, fragn) = {
                    // Allocate the tag that ties the fragments of this datagram together;
                    // the tag counter wraps around, so every transmitted datagram gets
                    // one. The tag is also saved for the other fragments that will be
                    // created when calling `poll` multiple times.
                    let tag = self.tag;
                    self.tag = self.tag.wrapping_add(1);
                    pkt.sixlowpan.datagram_tag = tag;

                    let frag1 = SixlowpanFragRepr::FirstFragment {
                        size: pkt.sixlowpan.datagram_size,
                        tag,
                    };
                    let fragn = SixlowpanFragRepr::Fragment {
                        size: pkt.sixlowpan.datagram_size,
                        tag,
                        offset: 0,
                    };

                    // We calculate how much data we can send in the first fragment and the
                    // other fragments. The eventual IPv6 sizes of these fragments need to be
                    // a multiple of eight (except for the last fragment) since the offset
                    // field in the fragment is an offset in multiples of 8 octets. This is
                    // explained in [RFC 4944 § 5.3].
                    //
                    // [RFC 4944 § 5.3]: https://datatracker.ietf.org/doc/html/rfc4944#section-5.3

                    let header_diff = uncompressed_size - compressed_size;
                    let frag1_size = (125 - ieee_len - frag1.buffer_len() + header_diff) / 8 * 8
                        - header_diff;

                    pkt.sixlowpan.fragn_size = (125 - ieee_len - fragn.buffer_len()) / 8 * 8;
                    pkt.sent_bytes = frag1_size;
                    pkt.sixlowpan.datagram_offset = frag1_size + header_diff;

                    (frag1_size, frag1, fragn)
                };

                tx_token.set_meta(meta);
                tx_token.consume(ieee_len + frag1.buffer_len() + frag1_size, |mut tx_buf| {
                    // Add the IEEE header.
                    let mut ieee_packet = Ieee802154Frame::new_unchecked(&mut tx_buf[..ieee_len]);
                    ieee_repr.emit(&mut ieee_packet);
                    tx_buf = &mut tx_buf[ieee_len..];

                    // Add the first fragment header
                    let mut frag1_packet = SixlowpanFragPacket::new_unchecked(&mut tx_buf);
                    frag1.emit(&mut frag1_packet);
                    tx_buf = &mut tx_buf[frag1.buffer_len()..];

                    // Add the buffer part.
                    tx_buf[..frag1_size].copy_from_slice(&pkt.buffer[..frag1_size]);
                });
            }

            #[cfg(not(feature = "proto-sixlowpan-fragmentation"))]
            {
                net_debug!(
                    "Enable the `proto-sixlowpan-fragmentation` feature for fragmentation support."
                );
                return;
            }
        } else {
            tx_token.set_meta(meta);

            // We don't need fragmentation, so we emit everything to the TX token. The
            // compression of the IPv6 packet happens inside the transmit callback, right
            // after the IEEE 802.15.4 header, since the compressed bytes go out on the
            // same frame buffer.
            tx_token.consume(total_size + ieee_len, |mut tx_buf| {
                let mut ieee_packet = Ieee802154Frame::new_unchecked(&mut tx_buf[..ieee_len]);
                ieee_repr.emit(&mut ieee_packet);
                tx_buf = &mut tx_buf[ieee_len..];

                // Compression stage: emit the compressed 6LoWPAN packet into the rest of
                // the transmit buffer.
                let checksum_caps = self.checksum_caps();

                let last_header = packet.payload.as_sixlowpan_next_header();
                let next_header = last_header;

                #[cfg(feature = "proto-ipv6-hbh")]
                let next_header = if packet.hop_by_hop.is_some() {
                    SixlowpanNextHeader::Compressed
                } else {
                    next_header
                };

                #[cfg(feature = "proto-ipv6-routing")]
                let next_header = if packet.routing.is_some() {
                    SixlowpanNextHeader::Compressed
                } else {
                    next_header
                };

                let iphc_repr = SixlowpanIphcRepr {
                    src_addr: packet.header.src_addr,
                    ll_src_addr: ieee_repr.src_addr,
                    dst_addr: packet.header.dst_addr,
                    ll_dst_addr: ieee_repr.dst_addr,
                    next_header,
                    hop_limit: packet.header.hop_limit,
                    ecn: None,
                    dscp: None,
                    flow_label: None,
                };

                iphc_repr.emit(&mut SixlowpanIphcPacket::new_unchecked(
                    &mut tx_buf[..iphc_repr.buffer_len()],
                ));
                tx_buf = &mut tx_buf[iphc_repr.buffer_len()..];

                // Emit the Hop-by-Hop header
                #[cfg(feature = "proto-ipv6-hbh")]
                if let Some(hbh) = packet.hop_by_hop {
                    #[allow(unused)]
                    let next_header = last_header;

                    #[cfg(feature = "proto-ipv6-routing")]
                    let next_header = if packet.routing.is_some() {
                        SixlowpanNextHeader::Compressed
                    } else {
                        last_header
                    };

                    let ext_hdr = SixlowpanExtHeaderRepr {
                        ext_header_id: SixlowpanExtHeaderId::HopByHopHeader,
                        next_header,
                        length: hbh.options.iter().map(|o| o.buffer_len()).sum::<usize>() as u8,
                    };
                    ext_hdr.emit(&mut SixlowpanExtHeaderPacket::new_unchecked(
                        &mut tx_buf[..ext_hdr.buffer_len()],
                    ));
                    tx_buf = &mut tx_buf[ext_hdr.buffer_len()..];

                    for opt in &hbh.options {
                        opt.emit(&mut Ipv6Option::new_unchecked(
                            &mut tx_buf[..opt.buffer_len()],
                        ));

                        tx_buf = &mut tx_buf[opt.buffer_len()..];
                    }
                }

                // Emit the Routing header
                #[cfg(feature = "proto-ipv6-routing")]
                if let Some(routing) = &packet.routing {
                    let ext_hdr = SixlowpanExtHeaderRepr {
                        ext_header_id: SixlowpanExtHeaderId::RoutingHeader,
                        next_header,
                        length: routing.buffer_len() as u8,
                    };
                    ext_hdr.emit(&mut SixlowpanExtHeaderPacket::new_unchecked(
                        &mut tx_buf[..ext_hdr.buffer_len()],
                    ));
                    tx_buf = &mut tx_buf[ext_hdr.buffer_len()..];

                    routing.emit(&mut Ipv6RoutingHeader::new_unchecked(
                        &mut tx_buf[..routing.buffer_len()],
                    ));
                    tx_buf = &mut tx_buf[routing.buffer_len()..];
                }

                match &mut packet.payload {
                    IpPayload::Icmpv6(icmp_repr) => {
                        icmp_repr.emit(
                            &packet.header.src_addr,
                            &packet.header.dst_addr,
                            &mut Icmpv6Packet::new_unchecked(
                                &mut tx_buf[..icmp_repr.buffer_len()],
                            ),
                            &checksum_caps,
                        );
                    }
                    #[cfg(any(feature = "socket-udp", feature = "socket-dns"))]
                    IpPayload::Udp(udp_repr, payload) => {
                        let udp_repr = SixlowpanUdpNhcRepr(*udp_repr);
                        udp_repr.emit(
                            &mut SixlowpanUdpNhcPacket::new_unchecked(
                                &mut tx_buf[..udp_repr.header_len() + payload.len()],
                            ),
                            &iphc_repr.src_addr,
                            &iphc_repr.dst_addr,
                            payload.len(),
                            |buf| buf.copy_from_slice(payload),
                            &checksum_caps,
                        );
                    }
                    #[cfg(feature = "socket-tcp")]
                    IpPayload::Tcp(tcp_repr) => {
                        tcp_repr.emit(
                            &mut TcpPacket::new_unchecked(&mut tx_buf[..tcp_repr.buffer_len()]),
                            &packet.header.src_addr.into(),
                            &packet.header.dst_addr.into(),
                            &checksum_caps,
                        );
                    }
                    #[cfg(feature = "socket-raw")]
                    IpPayload::Raw(_raw) => todo!(),

                    #[allow(unreachable_patterns)]
                    _ => unreachable!(),
                }
            });
        }
    }



    #[cfg(feature = "proto-sixlowpan-fragmentation")]
    pub(super) fn dispatch_sixlowpan_frag<Tx: TxToken>(
        &mut self,
        tx_token: Tx,
        ieee_repr: Ieee802154Repr,
        frag: &mut Fragmenter,
    ) {
        // Create the FRAG_N header.
        let fragn = SixlowpanFragRepr::Fragment {
            size: frag.sixlowpan.datagram_size,
            tag: frag.sixlowpan.datagram_tag,
            offset: (frag.sixlowpan.datagram_offset / 8) as u8,
        };

        let ieee_len = ieee_repr.buffer_len();
        let frag_size = (frag.packet_len - frag.sent_bytes).min(frag.sixlowpan.fragn_size);

        tx_token.consume(
            ieee_repr.buffer_len() + fragn.buffer_len() + frag_size,
            |mut tx_buf| {
                let mut ieee_packet = Ieee802154Frame::new_unchecked(&mut tx_buf[..ieee_len]);
                ieee_repr.emit(&mut ieee_packet);
                tx_buf = &mut tx_buf[ieee_len..];

                let mut frag_packet =
                    SixlowpanFragPacket::new_unchecked(&mut tx_buf[..fragn.buffer_len()]);
                fragn.emit(&mut frag_packet);
                tx_buf = &mut tx_buf[fragn.buffer_len()..];

                // Add the buffer part
                tx_buf[..frag_size].copy_from_slice(&frag.buffer[frag.sent_bytes..][..frag_size]);

                frag.sent_bytes += frag_size;
                frag.sixlowpan.datagram_offset += frag_size;
            },
        );
    }
}




#[cfg(test)]
#[cfg(all(feature = "proto-rpl", feature = "proto-ipv6-hbh"))]
mod tests {
    use super::*;

    static SIXLOWPAN_COMPRESSED_RPL_DAO: [u8; 99] = [
        0x61, 0xdc, 0x45, 0xcd, 0xab, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x03, 0x00,
        0x03, 0x00, 0x03, 0x00, 0x03, 0x00, 0x7e, 0xf7, 0x00, 0xe0, 0x3a, 0x06, 0x63, 0x04, 0x00,
        0x1e, 0x08, 0x00, 0x9b, 0x02, 0x3e, 0x63, 0x1e, 0x40, 0x00, 0xf1, 0xfd, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x02, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x05, 0x12, 0x00,
        0x80, 0xfd, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x00, 0x03, 0x00, 0x03,
        0x00, 0x03, 0x06, 0x14, 0x00, 0x00, 0x00, 0x1e, 0xfd, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01,
    ];

    static SIXLOWPAN_UNCOMPRESSED_RPL_DAO: [u8; 114] = [
        0x60, 0x00, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x40, 0xfd, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x03, 0x00, 0x03, 0x00, 0x03, 0x00, 0x03, 0xfd, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x02, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x3a, 0x00, 0x63, 0x04, 0x00,
        0x1e, 0x08, 0x00, 0x9b, 0x02, 0x3e, 0x63, 0x1e, 0x40, 0x00, 0xf1, 0xfd, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x02, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01, 0x05, 0x12, 0x00,
        0x80, 0xfd, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x03, 0x00, 0x03, 0x00, 0x03,
        0x00, 0x03, 0x06, 0x14, 0x00, 0x00, 0x00, 0x1e, 0xfd, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x01,
    ];

    #[test]
    fn test_sixlowpan_decompress_hop_by_hop_with_icmpv6() {
        let address_context = [SixlowpanAddressContext([
            0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
        ])];

        let ieee_frame = Ieee802154Frame::new_checked(&SIXLOWPAN_COMPRESSED_RPL_DAO).unwrap();
        let ieee_repr = Ieee802154Repr::parse(&ieee_frame).unwrap();

        let mut buffer = [0u8; 256];
        let len = InterfaceInner::sixlowpan_to_ipv6(
            &address_context,
            &ieee_repr,
            ieee_frame.payload().unwrap(),
            None,
            &mut buffer[..],
        )
        .unwrap();

        assert_eq!(&buffer[..len], &SIXLOWPAN_UNCOMPRESSED_RPL_DAO);
    }

    #[test]
    fn test_sixlowpan_compress_hop_by_hop_with_icmpv6() {
        let ieee_repr = Ieee802154Repr {
            frame_type: Ieee802154FrameType::Data,
            security_enabled: false,
            frame_pending: false,
            ack_request: true,
            sequence_number: Some(69),
            pan_id_compression: true,
            frame_version: Ieee802154FrameVersion::Ieee802154_2006,
            dst_pan_id: Some(Ieee802154Pan(43981)),
            dst_addr: Some(Ieee802154Address::Extended([0, 1, 0, 1, 0, 1, 0, 1])),
            src_pan_id: None,
            src_addr: Some(Ieee802154Address::Extended([0, 3, 0, 3, 0, 3, 0, 3])),
        };

        let mut ip_packet = PacketV6 {
            header: Ipv6Repr {
                src_addr: Ipv6Address::from_octets([
                    253, 0, 0, 0, 0, 0, 0, 0, 2, 3, 0, 3, 0, 3, 0, 3,
                ]),
                dst_addr: Ipv6Address::from_octets([
                    253, 0, 0, 0, 0, 0, 0, 0, 2, 1, 0, 1, 0, 1, 0, 1,
                ]),
                next_header: IpProtocol::Icmpv6,
                payload_len: 66,
                hop_limit: 64,
            },
            #[cfg(feature = "proto-ipv6-hbh")]
            hop_by_hop: None,
            #[cfg(feature = "proto-ipv6-fragmentation")]
            fragment: None,
            #[cfg(feature = "proto-ipv6-routing")]
            routing: None,
            payload: IpPayload::Icmpv6(Icmpv6Repr::Rpl(RplRepr::DestinationAdvertisementObject {
                rpl_instance_id: RplInstanceId::Global(30),
                expect_ack: false,
                sequence: 241,
                dodag_id: Some(Ipv6Address::from_octets([
                    253, 0, 0, 0, 0, 0, 0, 0, 2, 1, 0, 1, 0, 1, 0, 1,
                ])),
                options: &[],
            })),
        };

        let (total_size, _, _) = InterfaceInner::compressed_packet_size(&mut ip_packet, &ieee_repr);
        let mut buffer = vec![0u8; total_size];

        InterfaceInner::ipv6_to_sixlowpan(
            &ChecksumCapabilities::default(),
            ip_packet,
            &ieee_repr,
            &mut buffer[..total_size],
        );

        let result = [
            0x7e, 0x0, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x3, 0x0, 0x3, 0x0, 0x3, 0x0,
            0x3, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0, 0x1,
            0xe0, 0x3a, 0x6, 0x63, 0x4, 0x0, 0x1e, 0x3, 0x0, 0x9b, 0x2, 0x3e, 0x63, 0x1e, 0x40,
            0x0, 0xf1, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0,
            0x1, 0x5, 0x12, 0x0, 0x80, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x3, 0x0, 0x3,
            0x0, 0x3, 0x0, 0x3, 0x6, 0x14, 0x0, 0x0, 0x0, 0x1e, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x2, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0, 0x1,
        ];

        assert_eq!(&result, &result);
    }

    #[test]
    fn test_sixlowpan_compress_hop_by_hop_with_udp() {
        let ieee_repr = Ieee802154Repr {
            frame_type: Ieee802154FrameType::Data,
            security_enabled: false,
            frame_pending: false,
            ack_request: true,
            sequence_number: Some(69),
            pan_id_compression: true,
            frame_version: Ieee802154FrameVersion::Ieee802154_2006,
            dst_pan_id: Some(Ieee802154Pan(43981)),
            dst_addr: Some(Ieee802154Address::Extended([0, 1, 0, 1, 0, 1, 0, 1])),
            src_pan_id: None,
            src_addr: Some(Ieee802154Address::Extended([0, 3, 0, 3, 0, 3, 0, 3])),
        };

        let addr = Ipv6Address::from_octets([253, 0, 0, 0, 0, 0, 0, 0, 2, 3, 0, 3, 0, 3, 0, 3]);
        let parent_address =
            Ipv6Address::from_octets([253, 0, 0, 0, 0, 0, 0, 0, 2, 1, 0, 1, 0, 1, 0, 1]);

        let mut hbh_options = heapless::Vec::new();
        hbh_options
            .push(Ipv6OptionRepr::Rpl(RplHopByHopRepr {
                down: false,
                rank_error: false,
                forwarding_error: false,
                instance_id: RplInstanceId::from(0x1e),
                sender_rank: 0x300,
            }))
            .unwrap();

        let mut ip_packet = PacketV6 {
            header: Ipv6Repr {
                src_addr: addr,
                dst_addr: parent_address,
                next_header: IpProtocol::Icmpv6,
                payload_len: 66,
                hop_limit: 64,
            },
            #[cfg(feature = "proto-ipv6-hbh")]
            hop_by_hop: Some(Ipv6HopByHopRepr {
                options: hbh_options,
            }),
            #[cfg(feature = "proto-ipv6-fragmentation")]
            fragment: None,
            #[cfg(feature = "proto-ipv6-routing")]
            routing: None,
            payload: IpPayload::Icmpv6(Icmpv6Repr::Rpl(RplRepr::DestinationAdvertisementObject {
                rpl_instance_id: RplInstanceId::Global(30),
                expect_ack: false,
                sequence: 241,
                dodag_id: Some(Ipv6Address::from_octets([
                    253, 0, 0, 0, 0, 0, 0, 0, 2, 1, 0, 1, 0, 1, 0, 1,
                ])),
                options: &[
                    5, 18, 0, 128, 253, 0, 0, 0, 0, 0, 0, 0, 2, 3, 0, 3, 0, 3, 0, 3, 6, 20, 0, 0,
                    0, 30, 253, 0, 0, 0, 0, 0, 0, 0, 2, 1, 0, 1, 0, 1, 0, 1,
                ],
            })),
        };

        let (total_size, _, _) = InterfaceInner::compressed_packet_size(&mut ip_packet, &ieee_repr);
        let mut buffer = vec![0u8; total_size];

        InterfaceInner::ipv6_to_sixlowpan(
            &ChecksumCapabilities::default(),
            ip_packet,
            &ieee_repr,
            &mut buffer[..total_size],
        );

        let result = [
            0x7e, 0x0, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x3, 0x0, 0x3, 0x0, 0x3, 0x0,
            0x3, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0, 0x1,
            0xe0, 0x3a, 0x6, 0x63, 0x4, 0x0, 0x1e, 0x3, 0x0, 0x9b, 0x2, 0x3e, 0x63, 0x1e, 0x40,
            0x0, 0xf1, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0,
            0x1, 0x5, 0x12, 0x0, 0x80, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0x3, 0x0, 0x3,
            0x0, 0x3, 0x0, 0x3, 0x6, 0x14, 0x0, 0x0, 0x0, 0x1e, 0xfd, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0,
            0x0, 0x2, 0x1, 0x0, 0x1, 0x0, 0x1, 0x0, 0x1,
        ];

        assert_eq!(&buffer[..total_size], &result);
    }
}
