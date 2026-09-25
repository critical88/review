// MIT License
//
// Copyright (c) 2016-2017 xtaci
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

package smux

import (
	"sync/atomic"
)

// StreamAccounting is a point-in-time accounting view of a single stream,
// aimed at diagnosing stalled or unbalanced multiplexed connections.
type StreamAccounting struct {
	// ID is the stream's unique identifier.
	ID uint32

	// BytesRead is the cumulative number of bytes the reader has consumed.
	BytesRead uint64

	// BytesWritten is the cumulative number of bytes handed to the write path.
	BytesWritten uint64

	// BytesBuffered is the approximate number of payload bytes currently
	// buffered locally, waiting to be read.
	BytesBuffered int

	// Inflight is the number of bytes written but not yet reported consumed
	// by the peer (v2 only).
	Inflight uint32

	// PeerWindow is the last window size advertised by the peer (v2 only).
	PeerWindow uint32

	// ReadClosed reports whether the peer has half-closed its sending side.
	ReadClosed bool

	// WriteClosed reports whether the local writing side has been closed.
	WriteClosed bool

	// Closed reports whether the stream is fully closed.
	Closed bool
}

// StreamStats returns a point-in-time accounting view of the stream with the
// given stream id. The second return value is false if the session does not
// know a stream with this id, for example after it has been closed.
//
// Values are sampled without stopping the session, so they describe an
// approximation of the state at some recent instant.
func (s *Session) StreamStats(sid uint32) (StreamAccounting, bool) {
	s.streamLock.Lock()
	st := s.streams[sid]
	s.streamLock.Unlock()

	if st == nil {
		return StreamAccounting{}, false
	}

	var acc StreamAccounting
	acc.ID = st.id
	acc.BytesRead = uint64(st.numRead)
	acc.BytesWritten = uint64(st.numWritten)
	acc.Inflight = st.numWritten - st.peerConsumed
	acc.PeerWindow = st.peerWindow
	acc.BytesBuffered = st.bufferedBytes()

	select {
	case <-st.chFinEvent:
		acc.ReadClosed = true
	default:
	}

	select {
	case <-st.chWriteClosed:
		acc.WriteClosed = true
	default:
	}

	select {
	case <-st.die:
		acc.Closed = true
	default:
	}

	return acc, true
}

// ShaperBacklog reports how many write requests and payload bytes are currently
// queued by the traffic shaper across all streams.
//
// The shaper state is sampled without taking the shaper's lock, so a queue that
// is being drained concurrently may report fewer bytes than were queued at the
// start of the call. This keeps the sender and shaper loops from blocking on
// diagnostics. The walk tolerates the transient inconsistency between the
// scheduler's round-robin list and its per-stream heaps.
func (s *Session) ShaperBacklog() (queuedFrames uint64, queuedBytes uint64) {
	sq := s.sq

	// total number of queued write requests
	queuedFrames = uint64(atomic.LoadInt64(&sq.count))

	// walk the scheduler's round-robin list and sum the payload bytes
	// waiting in every stream's heap.
	for elem := sq.rrList.Front(); elem != nil; elem = elem.Next() {
		sid, _ := elem.Value.(uint32)
		h := sq.streams[sid]
		if h == nil {
			continue
		}
		for i := 0; i < h.Len(); i++ {
			queuedBytes += uint64(len((*h)[i].frame.data))
		}
	}
	return queuedFrames, queuedBytes
}
