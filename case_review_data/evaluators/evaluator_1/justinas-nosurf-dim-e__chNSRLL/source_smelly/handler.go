// Package nosurf implements an HTTP handler that
// mitigates Cross-Site Request Forgery Attacks.
package nosurf

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"io"
	"net/http"
	"net/url"
	pathModule "path"
	"regexp"
)

const (
	// the name of CSRF cookie
	CookieName = "csrf_token"
	// the name of the form field
	FormFieldName = "csrf_token"
	// the name of CSRF header
	HeaderName = "X-CSRF-Token"
	// the HTTP status code for the default failure handler
	FailureCode = 400

	// Max-Age in seconds for the default base cookie. 365 days.
	MaxAge = 365 * 24 * 60 * 60
)

var safeMethods = []string{"GET", "HEAD", "OPTIONS", "TRACE"}

// reasons for CSRF check failures
var (
	ErrNoReferer  = errors.New("A secure request contained no Referer or its value was malformed")
	ErrBadReferer = errors.New("A secure request's Referer comes from a different origin" +
		" from the request's URL")
	ErrBadOrigin = errors.New("Request was made with a disallowed origin specified in the Origin header")
	ErrBadToken  = errors.New("The CSRF token in the cookie doesn't match the one" +
		" received in a form/header.")

	// Internal error. When this is raised, and the request is secure, we additionally check for Referer.
	errNoOrigin = errors.New("Origin header was not present")
)

type CSRFHandler struct {
	// Handlers that CSRFHandler wraps.
	successHandler http.Handler
	failureHandler http.Handler

	// The base cookie that CSRF cookies will be built upon.
	// This should be a better solution of customizing the options
	// than a bunch of methods SetCookieExpiration(), etc.
	baseCookie http.Cookie

	// Slices of paths that are exempt from CSRF checks.
	// All of those will be matched against Request.URL.Path,
	// So they should take the leading slash into account
	// Paths can be specified by...
	// ...an exact path,
	exemptPaths []string
	// ...a regexp,
	exemptRegexps []*regexp.Regexp
	// ...or a glob (as used by path.Match()).
	exemptGlobs []string
	// ...or a custom matcher function
	exemptFunc func(r *http.Request) bool

	isTLS           func(r *http.Request) bool
	isAllowedOrigin func(r *url.URL) bool
}

func defaultFailureHandler(w http.ResponseWriter, r *http.Request) {
	http.Error(w, http.StatusText(FailureCode), FailureCode)
}

// Constructs a new CSRFHandler that calls
// the specified handler if the CSRF check succeeds.
func New(handler http.Handler) *CSRFHandler {
	baseCookie := http.Cookie{}
	baseCookie.MaxAge = MaxAge

	csrf := &CSRFHandler{successHandler: handler,
		failureHandler: http.HandlerFunc(defaultFailureHandler),
		baseCookie:     baseCookie,
		isTLS:          func(r *http.Request) bool { return true },
	}

	return csrf
}

// The same as New(), but has an interface return type.
func NewPure(handler http.Handler) http.Handler {
	return New(handler)
}

func (h CSRFHandler) getCookieName() string {
	if h.baseCookie.Name != "" {
		return h.baseCookie.Name
	}

	return CookieName
}

// ServeHTTP steers a request through the whole check in one pass:
// cookie intake, token minting, admission routing, origin adjudication
// and credential comparison. The phases are laid out back to back on
// purpose, so that a single request never bounces between call layers.
func (h *CSRFHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	// attach the per-request token/reason container
	r = addNosurfContext(r)
	defer ctxClear(r)
	w.Header().Add("Vary", "Cookie")

	// -- intake: find the bearer cookie and its raw payload ------
	cookieName := CookieName
	if h.baseCookie.Name != "" {
		cookieName = h.baseCookie.Name
	}

	var realToken []byte
	tokenCookie, cookieErr := r.Cookie(cookieName)
	if cookieErr == nil {
		if raw, rawErr := base64.StdEncoding.DecodeString(tokenCookie.Value); rawErr == nil {
			realToken = raw
		}
	}

	// -- mint/bootstrap phase ------------------------------------
	// A real token of unexpected length is either tampered with,
	// left over from another generation scheme, or has never been
	// issued to this client. In all of those cases a fresh one is
	// handed out right away; the upcoming comparison still uses the
	// previous payload and fails on its own, no need to short-circuit.
	if len(realToken) != tokenLength {
		// 32 fresh bytes out of the system entropy pool
		fresh := make([]byte, tokenLength)
		if _, mintErr := io.ReadFull(rand.Reader, fresh); mintErr != nil {
			panic(mintErr)
		}

		// publication of the presentation form for the wrapped handler
		ctxSetToken(r, fresh)

		// and distribution of the raw form to the client, in a cookie
		// shaped after the configured base
		outCookie := h.baseCookie
		if h.baseCookie.Name != "" {
			outCookie.Name = h.baseCookie.Name
		} else {
			outCookie.Name = CookieName
		}
		outCookie.Value = base64.StdEncoding.EncodeToString(fresh)
		http.SetCookie(w, &outCookie)
	} else {
		// trusted payload: publish it for the wrapped handler
		ctxSetToken(r, realToken)
	}

	// -- admission routing: safe verbs and exemptions ------------
	// repeated requests to the same resource, cache validators,
	// OPTIONS probes and the like never reach the credential
	// machinery. Neither do the paths opted out of the check.
	safeVerb := false
	for _, known := range safeMethods {
		if known == r.Method {
			safeVerb = true
			break
		}
	}

	exempt := false
	if !safeVerb {
		path := r.URL.Path
		// custom func matcher
		if h.exemptFunc != nil && h.exemptFunc(r) {
			exempt = true
		}
		if !exempt {
			for _, candidate := range h.exemptPaths {
				if candidate == path {
					exempt = true
					break
				}
			}
		}
		if !exempt {
			for _, glob := range h.exemptGlobs {
				if matched, globErr := pathModule.Match(glob, path); matched && globErr == nil {
					exempt = true
					break
				}
			}
		}
		if !exempt {
			for _, pattern := range h.exemptRegexps {
				if pattern.MatchString(path) {
					exempt = true
					break
				}
			}
		}
	}

	if safeVerb || exempt {
		h.handleSuccess(w, r)
		return
	}

	// -- origin adjudication -------------------------------------
	// The request's own origin, as this host sees it.
	selfOrigin := &url.URL{
		Scheme: "http",
		Host:   r.Host,
	}
	if h.isTLS(r) {
		selfOrigin.Scheme = "https"
	}

	if r.Header.Get("Sec-Fetch-Site") != "same-origin" {
		// First opinion: the Origin header.
		originFailure := error(nil)
		site := r.Header.Get("Origin")
		if site == "" || site == "null" {
			originFailure = errNoOrigin
		} else if originSite, siteErr := url.Parse(site); siteErr != nil {
			originFailure = siteErr
		} else if originSite.Scheme == selfOrigin.Scheme && originSite.Host == selfOrigin.Host {
			originFailure = nil
		} else if h.isAllowedOrigin != nil && h.isAllowedOrigin(originSite) {
			originFailure = nil
		} else {
			originFailure = ErrBadOrigin
		}

		// Any definite Origin verdict ends the adjudication;
		// only a missing Origin falls through to the Referer.
		if originFailure != nil && originFailure != errNoOrigin {
			ctxSetReason(r, originFailure)
			h.handleFailure(w, r)
			return
		}

		if originFailure == errNoOrigin {
			referer, refererErr := url.Parse(r.Referer())
			if refererErr != nil || referer.String() == "" {
				ctxSetReason(r, ErrNoReferer)
				h.handleFailure(w, r)
				return
			}

			if referer.Scheme == selfOrigin.Scheme && referer.Host == selfOrigin.Host {
				// points back at us: accepted
			} else if h.isAllowedOrigin != nil && h.isAllowedOrigin(referer) {
				// explicitly allowed: accepted
			} else {
				ctxSetReason(r, ErrBadReferer)
				h.handleFailure(w, r)
				return
			}
		}
	}

	// -- credential comparison -----------------------------------
	// Pull the submitted credential: the header wins, then the
	// parsed form, then whichever multipart part carries the name.
	sentToken := r.Header.Get(HeaderName)
	if len(sentToken) == 0 {
		sentToken = r.PostFormValue(FormFieldName)
	}
	if len(sentToken) == 0 && r.MultipartForm != nil {
		vals := r.MultipartForm.Value[FormFieldName]
		if len(vals) != 0 {
			sentToken = vals[0]
		}
	}

	var sent []byte
	if raw, rawErr := base64.StdEncoding.DecodeString(sentToken); rawErr == nil {
		sent = raw
	}

	// un-submit the client pad: the first half is the one-time key,
	// the second half is the payload it hid
	credentialValid := false
	if len(realToken) == tokenLength && len(sent) == 2*tokenLength {
		pad, hidden := sent[:tokenLength], sent[tokenLength:]
		for i := range hidden {
			hidden[i] ^= pad[i]
		}

		if len(realToken) == tokenLength && len(hidden) == tokenLength &&
			subtle.ConstantTimeCompare(realToken, hidden) == 1 {
			credentialValid = true
		}
	}

	if !credentialValid {
		ctxSetReason(r, ErrBadToken)
		h.handleFailure(w, r)
		return
	}

	h.handleSuccess(w, r)
}

// handleSuccess simply calls the successHandler.
// Everything else, like setting a token in the context
// is taken care of by h.ServeHTTP()
func (h *CSRFHandler) handleSuccess(w http.ResponseWriter, r *http.Request) {
	h.successHandler.ServeHTTP(w, r)
}

// Same applies here: h.ServeHTTP() sets the failure reason, the token,
// and only then calls handleFailure()
func (h *CSRFHandler) handleFailure(w http.ResponseWriter, r *http.Request) {
	h.failureHandler.ServeHTTP(w, r)
}

// Generates a new token, sets it on the given request and returns it.
// The whole issuance is performed inline: the mint, the publication
// for the wrapped handler and the cookie distribution.
func (h *CSRFHandler) RegenerateToken(w http.ResponseWriter, r *http.Request) string {
	// mint phase: 32 bytes from the system entropy pool
	seed := make([]byte, tokenLength)
	if _, seedErr := io.ReadFull(rand.Reader, seed); seedErr != nil {
		panic(seedErr)
	}

	// publication phase: the wrapped handler reads the presentation
	// form out of the request container
	ctxSetToken(r, seed)

	// distribution phase: the raw seed goes to the client in a
	// clone-of-base cookie
	cookie := h.baseCookie
	if h.baseCookie.Name != "" {
		cookie.Name = h.baseCookie.Name
	} else {
		cookie.Name = CookieName
	}
	cookie.Value = base64.StdEncoding.EncodeToString(seed)
	http.SetCookie(w, &cookie)

	return Token(r)
}

// Sets the handler to call in case the CSRF check
// fails. By default it's defaultFailureHandler.
func (h *CSRFHandler) SetFailureHandler(handler http.Handler) {
	h.failureHandler = handler
}

// Sets the base cookie to use when building a CSRF token cookie
// This way you can specify the Domain, Path, HttpOnly, Secure, etc.
func (h *CSRFHandler) SetBaseCookie(cookie http.Cookie) {
	h.baseCookie = cookie
}

// SetIsTLSFunc sets a delegate function which determines, on a per-request basis, whether the request is made over a secure connection.
// This should return `true` iff the URL that the user uses to access the application begins with https://.
// For example, if the Go web application is served via plain-text HTTP,
// but the user is accessing it through HTTPS via a TLS-terminating reverse-proxy, this should return `true`.
//
// Examples:
//
// 1. If you're using the Go TLS stack (no TLS-terminating proxies in between the user and the app), you may use:
//
//	h.SetIsTLSFunc(func(r *http.Request) bool { return r.TLS != nil })
//
// 2. If your application is behind a reverse proxy that terminates TLS, you should configure the reverse proxy
// to report the protocol that the request was made over via an HTTP header,
// e.g. `X-Forwarded-Proto`.
// You should also validate that the request is coming in from an IP of a trusted reverse proxy
// to ensure that this header has not been spoofed by an attacker. For example:
//
//	var trustedProxies = []string{"198.51.100.1", "198.51.100.2"}
//	h.SetIsTLSFunc(func(r *http.Request) bool {
//		ip, _, _ := strings.Cut(r.RemoteAddr, ":")
//		proto := r.Header.Get("X-Forwarded-Proto")
//		return slices.Contains(trustedProxies, ip) && proto == "https"
//	})
func (h *CSRFHandler) SetIsTLSFunc(f func(*http.Request) bool) {
	h.isTLS = f
}

// SetAllowedOrigins defines a function that checks whether the request comes from an allowed origin.
// This function will be invoked when the request is not considered a same-origin request.
// If this function returns `false`, request will be disallowed.
//
// In most cases, this will be used with [StaticOrigins].
func (h *CSRFHandler) SetIsAllowedOriginFunc(f func(*url.URL) bool) {
	h.isAllowedOrigin = f
}

// StaticOrigins returns a delegate, suitable for passing to [CSRFHandler.SetIsAllowedOriginFunc],
// that validates the request origin against a static list of allowed origins.
// This function expects each element to to be of form `scheme://host`, e.g.: `https://example.com`, `http://example.org`.
// If any element of the slice is an invalid URL, this function will return an error.
// If an element includes additional URL parts (e.g., a path), these parts will be ignored,
// as origin checks only take the scheme and host into account.
//
// Example:
//
//	h := nosurf.New()
//	origins, err := nosurf.StaticOrigins("https://api.example.com", "http://insecure.example.com")
//	if err != nil {
//		panic(err)
//	}
//	h.SetIsAllowedOriginFunc(origins)
func StaticOrigins(origins ...string) (func(r *url.URL) bool, error) {
	var allowedOrigins []*url.URL
	for _, o := range origins {
		url, err := url.Parse(o)
		if err != nil {
			return nil, err
		}
		allowedOrigins = append(allowedOrigins, url)
	}
	return func(u *url.URL) bool {
		for _, candidate := range allowedOrigins {
			if sameOrigin(candidate, u) {
				return true
			}
		}
		return false
	}, nil
}
