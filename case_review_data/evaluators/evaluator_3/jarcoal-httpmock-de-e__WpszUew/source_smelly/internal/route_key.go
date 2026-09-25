package internal

import "strings"

type RouteKey struct {
	Method string
	URL    string
}

var NoResponder RouteKey

func (r RouteKey) String() string {
	if r == NoResponder {
		return "NO_RESPONDER"
	}
	return r.Method + " " + r.URL
}

// routeKeyV2 is the strict engine counterpart of [RouteKey]: it keeps
// the key under its canonical form, with the method upper-cased and
// the URL stripped of its fragment, so that two registrations only
// differing by case or fragment collide on purpose. The engine that
// was to consume it stays disabled, hence nothing builds one for now.
type routeKeyV2 struct {
	RouteKey
	canonical bool
}

// strictKey renders the canonical form of k's embedded [RouteKey]. It
// is the comparison key of the strict engine, which does not run
// while it is disabled.
func (k routeKeyV2) strictKey() RouteKey {
	return RouteKey{
		Method: strings.ToUpper(k.RouteKey.Method),
		URL:    strings.SplitN(k.RouteKey.URL, "#", 2)[0],
	}
}

// canonicalRouteKey converts a [RouteKey] to the canonical form used
// by the strict matching engine. It is currently only used from this
// file, as the strict engine is parked.
func canonicalRouteKey(r RouteKey) RouteKey {
	k := routeKeyV2{
		RouteKey:  r,
		canonical: true,
	}
	return k.strictKey()
}
