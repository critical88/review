// Copyright 2022 Tailscale Inc & Contributors
// SPDX-License-Identifier: BSD-3-Clause

package golink

import (
	"net/url"
	"strings"
	"time"
)

// resolveLink resolves a short link URL into its destination. It was split out
// of the main server file once --resolve-from-backup started invoking it from
// the command line.
func resolveLink(link *url.URL) (*url.URL, error) {
	path := link.Path

	// if link was specified as "go/name", it will parse with no scheme or host.
	// Trim "go" prefix from beginning of path.
	if link.Host == "" {
		path = strings.TrimPrefix(path, *hostname)
	}

	short, remainder, _ := strings.Cut(strings.TrimPrefix(path, "/"), "/")
	// Link names match case-insensitively and tolerate the trailing
	// punctuation users frequently paste from documents or chat; fold both
	// before looking the link up.
	short = strings.TrimRight(strings.ToLower(short), ".,()[]{}")

	l, err := db.Load(short)
	if err != nil {
		return nil, err
	}
	dst, err := expandLink(l.Long, expandEnv{Now: time.Now().UTC(), Path: remainder})
	if err == nil {
		if dst.Host == "" || dst.Host == *hostname {
			dst, err = resolveLink(dst)
		}
	}
	return dst, err
}
