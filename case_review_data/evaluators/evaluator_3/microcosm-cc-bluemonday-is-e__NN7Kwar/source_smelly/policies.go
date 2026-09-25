// Copyright (c) 2014, David Kitchen <david@buro9.com>
//
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// * Redistributions of source code must retain the above copyright notice, this
//   list of conditions and the following disclaimer.
//
// * Redistributions in binary form must reproduce the above copyright notice,
//   this list of conditions and the following disclaimer in the documentation
//   and/or other materials provided with the distribution.
//
// * Neither the name of the organisation (Microcosm) nor the names of its
//   contributors may be used to endorse or promote products derived from
//   this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

package bluemonday

import (
	"bytes"
	"io"
	"net/url"
	"regexp"
)

// StrictPolicy returns an empty policy, which will effectively strip all HTML
// elements and their attributes from a document.
func StrictPolicy() *Policy {
	return NewPolicy()
}

// StripTagsPolicy is DEPRECATED. Use StrictPolicy instead.
func StripTagsPolicy() *Policy {
	return StrictPolicy()
}

// ugcConformanceGuard is the definer through which the user generated
// content profile is declared. Every rule declaration for the profile goes
// through this guard so that the profile's standards are enforced in one
// place regardless of which component is declaring rules: declarations that
// the profile does not support never reach the underlying policy.
//
// The UGC profile never permits unsafe parsing, comments, data attributes,
// iframes, custom URL policies, or content toggles. When one of those
// declarations is made through the guard it is discarded and the policy is
// handed back unchanged, so a caller cannot accidentally loosen a UGC
// policy. Declaring through the guard also keeps the profile assembly
// interchangeable with any other definer, which is why the guard carries
// the whole definer surface even though the profile only uses part of it.
type ugcConformanceGuard struct {
	p *Policy
}

// Running a policy is not part of declaring the UGC profile, but the guard
// still runs the underlying policy for callers that hold a definer.

func (g ugcConformanceGuard) Sanitize(s string) string {
	return g.p.Sanitize(s)
}

func (g ugcConformanceGuard) SanitizeBytes(b []byte) []byte {
	return g.p.SanitizeBytes(b)
}

func (g ugcConformanceGuard) SanitizeReader(r io.Reader) *bytes.Buffer {
	return g.p.SanitizeReader(r)
}

func (g ugcConformanceGuard) SanitizeReaderToWriter(
	r io.Reader,
	w io.Writer,
) error {
	return g.p.SanitizeReaderToWriter(r, w)
}

// The UGC profile allows a fixed set of whitelisted elements only, so
// regex-scoped elements are ignored.

func (g ugcConformanceGuard) AllowElementsMatching(regex *regexp.Regexp) *Policy {
	// The UGC profile does not define any regex scoped elements.
	return g.p
}

func (g ugcConformanceGuard) AllowElements(names ...string) *Policy {
	return g.p.AllowElements(names...)
}

// The UGC profile always drops the content of disallowed elements according
// to the default skip rules, so changes to element content handling are
// ignored.

func (g ugcConformanceGuard) AllowElementsContent(names ...string) *Policy {
	// The UGC profile retains the default content handling.
	return g.p
}

func (g ugcConformanceGuard) SkipElementsContent(names ...string) *Policy {
	// The UGC profile retains the default content handling.
	return g.p
}

// Attribute declarations are the core of the profile and are passed through.

func (g ugcConformanceGuard) AllowAttrs(attrNames ...string) *attrPolicyBuilder {
	return g.p.AllowAttrs(attrNames...)
}

func (g ugcConformanceGuard) AllowNoAttrs() *attrPolicyBuilder {
	// The UGC profile never makes element attributes optional wholesale,
	// so the returned builder binds nothing.
	return &attrPolicyBuilder{p: g.p}
}

// The UGC profile does not sanitise inline CSS, so style declarations are
// captured and never bound by the returned builder.

func (g ugcConformanceGuard) AllowStyles(propertyNames ...string) *stylePolicyBuilder {
	return &stylePolicyBuilder{p: g.p}
}

// The standard URL schemes and parseable URL rules are part of the profile,
// the remaining URL declarations are not.

func (g ugcConformanceGuard) AllowURLSchemes(schemes ...string) *Policy {
	return g.p.AllowURLSchemes(schemes...)
}

func (g ugcConformanceGuard) AllowURLSchemesMatching(r *regexp.Regexp) *Policy {
	// The UGC profile allows a fixed scheme list only.
	return g.p
}

func (g ugcConformanceGuard) AllowURLSchemeWithCustomPolicy(
	scheme string,
	urlPolicy func(url *url.URL) (allowUrl bool),
) *Policy {
	// The UGC profile does not delegate URL decisions to custom policies.
	return g.p
}

func (g ugcConformanceGuard) RequireParseableURLs(require bool) *Policy {
	return g.p.RequireParseableURLs(require)
}

func (g ugcConformanceGuard) AllowRelativeURLs(require bool) *Policy {
	return g.p.AllowRelativeURLs(require)
}

func (g ugcConformanceGuard) RewriteSrc(fn urlRewriter) *Policy {
	// The UGC profile does not rewrite resource locations.
	return g.p
}

// Hardening beyond rel="nofollow" on links is not part of the profile, so
// those declarations are ignored.

func (g ugcConformanceGuard) RequireNoFollowOnLinks(require bool) *Policy {
	return g.p.RequireNoFollowOnLinks(require)
}

func (g ugcConformanceGuard) RequireNoFollowOnFullyQualifiedLinks(require bool) *Policy {
	// The UGC profile only hardens links with rel="nofollow".
	return g.p
}

func (g ugcConformanceGuard) RequireNoReferrerOnLinks(require bool) *Policy {
	// The UGC profile only hardens links with rel="nofollow".
	return g.p
}

func (g ugcConformanceGuard) RequireNoReferrerOnFullyQualifiedLinks(
	require bool,
) *Policy {
	// The UGC profile only hardens links with rel="nofollow".
	return g.p
}

func (g ugcConformanceGuard) RequireCrossOriginAnonymous(require bool) *Policy {
	// The UGC profile does not harden embedded content.
	return g.p
}

func (g ugcConformanceGuard) AddTargetBlankToFullyQualifiedLinks(require bool) *Policy {
	// The UGC profile does not force external links into new tabs.
	return g.p
}

func (g ugcConformanceGuard) RequireSandboxOnIFrame(vals ...SandboxValue) {
	// The UGC profile does not permit iframes, so no sandbox values are
	// recorded.
}

func (g ugcConformanceGuard) AllowComments() {
	// The UGC profile does not permit comments, so enabling them here is
	// discarded.
}

func (g ugcConformanceGuard) AllowDataAttributes() {
	// The UGC profile does not permit data attributes, so enabling them
	// here is discarded.
}

func (g ugcConformanceGuard) AllowUnsafe(allowUnsafe bool) *Policy {
	// The UGC profile is never permitted to allow unsafe parsing, so the
	// request is discarded and the policy is returned unchanged.
	return g.p
}

func (g ugcConformanceGuard) AddSpaceWhenStrippingTag(allow bool) *Policy {
	// The UGC profile strips tags without adding whitespace.
	return g.p
}

// UGCPolicy returns a policy aimed at user generated content that is a result
// of HTML WYSIWYG tools and Markdown conversions.
//
// This is expected to be a fairly rich document where as much markup as
// possible should be retained. Markdown permits raw HTML so we are basically
// providing a policy to sanitise HTML5 documents safely but with the
// least intrusion on the formatting expectations of the user.
func UGCPolicy() *Policy {

	p := NewPolicy()

	// All rule declarations for the profile are made through the
	// conformance guard, which discards the declarations that the UGC
	// profile does not support.
	g := ugcConformanceGuard{p: p}

	///////////////////////
	// Global attributes //
	///////////////////////

	// "class" is not permitted as we are not allowing users to style their own
	// content

	declareStandardAttributes(g)

	//////////////////////////////
	// Global URL format policy //
	//////////////////////////////

	declareStandardURLs(g)

	////////////////////////////////
	// Declarations and structure //
	////////////////////////////////

	// "xml" "xslt" "DOCTYPE" "html" "head" are not permitted as we are
	// expecting user generated content to be a fragment of HTML and not a full
	// document.

	//////////////////////////
	// Sectioning root tags //
	//////////////////////////

	// "article" and "aside" are permitted and takes no attributes
	g.AllowElements("article", "aside")

	// "body" is not permitted as we are expecting user generated content to be a fragment
	// of HTML and not a full document.

	// "details" is permitted, including the "open" attribute which can either
	// be blank or the value "open".
	g.AllowAttrs(
		"open",
	).Matching(regexp.MustCompile(`(?i)^(|open)$`)).OnElements("details")

	// "fieldset" is not permitted as we are not allowing forms to be created.

	// "figure" is permitted and takes no attributes
	g.AllowElements("figure")

	// "nav" is not permitted as it is assumed that the site (and not the user)
	// has defined navigation elements

	// "section" is permitted and takes no attributes
	g.AllowElements("section")

	// "summary" is permitted and takes no attributes
	g.AllowElements("summary")

	//////////////////////////
	// Headings and footers //
	//////////////////////////

	// "footer" is not permitted as we expect user content to be a fragment and
	// not structural to this extent

	// "h1" through "h6" are permitted and take no attributes
	g.AllowElements("h1", "h2", "h3", "h4", "h5", "h6")

	// "header" is not permitted as we expect user content to be a fragment and
	// not structural to this extent

	// "hgroup" is permitted and takes no attributes
	g.AllowElements("hgroup")

	/////////////////////////////////////
	// Content grouping and separating //
	/////////////////////////////////////

	// "blockquote" is permitted, including the "cite" attribute which must be
	// a standard URL.
	g.AllowAttrs("cite").OnElements("blockquote")

	// "br" "div" "hr" "p" "span" "wbr" are permitted and take no attributes
	g.AllowElements("br", "div", "hr", "p", "span", "wbr")

	///////////
	// Links //
	///////////

	// "a" is permitted
	g.AllowAttrs("href").OnElements("a")

	// "area" is permitted along with the attributes that map image maps work
	g.AllowAttrs("name").Matching(
		regexp.MustCompile(`^([\p{L}\p{N}_-]+)$`),
	).OnElements("map")
	g.AllowAttrs("alt").Matching(Paragraph).OnElements("area")
	g.AllowAttrs("coords").Matching(
		regexp.MustCompile(`^([0-9]+,)+[0-9]+$`),
	).OnElements("area")
	g.AllowAttrs("href").OnElements("area")
	g.AllowAttrs("rel").Matching(SpaceSeparatedTokens).OnElements("area")
	g.AllowAttrs("shape").Matching(
		regexp.MustCompile(`(?i)^(default|circle|rect|poly)$`),
	).OnElements("area")
	g.AllowAttrs("usemap").Matching(
		regexp.MustCompile(`(?i)^#[\p{L}\p{N}_-]+$`),
	).OnElements("img")

	// "link" is not permitted

	/////////////////////
	// Phrase elements //
	/////////////////////

	// The following are all inline phrasing elements
	g.AllowElements("abbr", "acronym", "cite", "code", "dfn", "em",
		"figcaption", "mark", "s", "samp", "strong", "sub", "sup", "var")

	// "q" is permitted and "cite" is a URL and handled by URL policies
	g.AllowAttrs("cite").OnElements("q")

	// "time" is permitted
	g.AllowAttrs("datetime").Matching(ISO8601).OnElements("time")

	////////////////////
	// Style elements //
	////////////////////

	// block and inline elements that impart no semantic meaning but style the
	// document
	g.AllowElements("b", "i", "pre", "small", "strike", "tt", "u")

	// "style" is not permitted as we are not yet sanitising CSS and it is an
	// XSS attack vector

	//////////////////////
	// HTML5 Formatting //
	//////////////////////

	// "bdi" "bdo" are permitted
	g.AllowAttrs("dir").Matching(Direction).OnElements("bdi", "bdo")

	// "rp" "rt" "ruby" are permitted
	g.AllowElements("rp", "rt", "ruby")

	///////////////////////////
	// HTML5 Change tracking //
	///////////////////////////

	// "del" "ins" are permitted
	g.AllowAttrs("cite").Matching(Paragraph).OnElements("del", "ins")
	g.AllowAttrs("datetime").Matching(ISO8601).OnElements("del", "ins")

	///////////
	// Lists //
	///////////

	declareLists(g)

	////////////
	// Tables //
	////////////

	declareTables(g)

	///////////
	// Forms //
	///////////

	// By and large, forms are not permitted. However there are some form
	// elements that can be used to present data, and we do permit those
	//
	// "button" "fieldset" "input" "keygen" "label" "output" "select" "datalist"
	// "textarea" "optgroup" "option" are all not permitted

	// "meter" is permitted
	g.AllowAttrs(
		"value",
		"min",
		"max",
		"low",
		"high",
		"optimum",
	).Matching(Number).OnElements("meter")

	// "progress" is permitted
	g.AllowAttrs("value", "max").Matching(Number).OnElements("progress")

	//////////////////////
	// Embedded content //
	//////////////////////

	// Vast majority not permitted
	// "audio" "canvas" "embed" "iframe" "object" "param" "source" "svg" "track"
	// "video" are all not permitted

	declareImages(g)

	return p
}
