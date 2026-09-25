// Copyright 2013 by Dobrosław Żybort. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

package slug

import (
	"bytes"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"
)

var (
	// CustomSub stores custom substitution map
	CustomSub map[string]string
	// CustomRuneSub stores custom rune substitution map
	CustomRuneSub map[rune]string

	// MaxLength stores maximum slug length.
	// By default slugs aren't shortened.
	// If MaxLength is smaller than length of the first word, then returned
	// slug will contain only substring from the first word truncated
	// after MaxLength.
	MaxLength int

	// EnableSmartTruncate defines if cutting with MaxLength is smart.
	// Smart algorithm will cat slug after full word.
	// Default is true.
	EnableSmartTruncate = true

	// Lowercase defines if the resulting slug is transformed to lowercase.
	// Default is true.
	Lowercase = true

	// DisableMultipleDashTrim defines if multiple dashes should be preserved.
	// Default is false (multiple dashes will be replaced with single dash).
	DisableMultipleDashTrim = false

	// DisableEndsTrim defines if the slug should keep leading and trailing
	// dashes and underscores. Default is false (trim enabled).
	DisableEndsTrim = false

	// Append timestamp to the end in order to make slug unique
	// Default is false
	AppendTimestamp = false

	regexpNonAuthorizedChars = regexp.MustCompile("[^a-zA-Z0-9-_]")
	regexpMultipleDashes     = regexp.MustCompile("-+")
)

//=============================================================================

// Make returns slug generated from provided string. Will use "en" as language
// substitution.
func Make(s string) (slug string) {
	return MakeLang(s, "en")
}

// MakeLang returns slug generated from provided string and will use provided
// language for chars substitution.
func MakeLang(s string, lang string) (slug string) {
	slug = strings.TrimSpace(s)

	// Advance every registered stage through the phases of the pipeline.
	slug = applyStages(slug, lang)

	return slug
}

// Substitute returns string with superseded all substrings from
// provided substitution map. Substitution map will be applied in alphabetic
// order. Many passes, on one substitution another one could apply.
func Substitute(s string, sub map[string]string) (buf string) {
	buf = s
	keys := make([]string, 0, len(sub))
	for k := range sub {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	for _, key := range keys {
		buf = strings.Replace(buf, key, sub[key], -1)
	}
	return
}

// SubstituteRune substitutes string chars with provided rune
// substitution map. One pass.
func SubstituteRune(s string, sub map[rune]string) string {
	var buf bytes.Buffer
	for _, c := range s {
		if d, ok := sub[c]; ok {
			buf.WriteString(d)
		} else {
			buf.WriteRune(c)
		}
	}
	return buf.String()
}

func smartTruncate(text string) string {
	if len(text) <= MaxLength {
		return text
	}

	// If slug is too long, we need to find the last '-' before MaxLength, and
	// we cut there.
	// If we don't find any, we have only one word, and we cut at MaxLength.
	for i := MaxLength; i >= 0; i-- {
		if text[i] == '-' {
			return text[:i]
		}
	}
	return text[:MaxLength]
}

// timestamp returns current timestamp as string
func timestamp() string {
	return strconv.FormatInt(time.Now().Unix(), 10)
}

// IsSlug returns True if provided text does not contain white characters,
// punctuation, all letters are lower case and only from ASCII range.
// It could contain `-` and `_` but not at the beginning or end of the text.
// It should be in range of the MaxLength var if specified.
// All output from slug.Make(text) should pass this test.
func IsSlug(text string) bool {
	return noopStage{}.IsValidSlug(text)
}

//=============================================================================
// Pipeline stages for the options kept in this file.

// caseFoldingStage enforces the Lowercase option during the case phase.
type caseFoldingStage struct{ noopStage }

func (caseFoldingStage) StageName() string { return "case folding" }

// ApplyCase lowers the text when the Lowercase option is enabled.
func (caseFoldingStage) ApplyCase(s string) string {
	if Lowercase {
		return strings.ToLower(s)
	}
	return s
}

// lengthGuardStage enforces MaxLength during both length phases, with or
// without word boundaries.
type lengthGuardStage struct{ noopStage }

func (lengthGuardStage) StageName() string { return "length guard" }

// Truncate cuts the text short when the MaxLength option is used with smart
// truncation disabled.
func (lengthGuardStage) Truncate(s string) string {
	if !EnableSmartTruncate && len(s) >= MaxLength {
		return s[:MaxLength]
	}
	return s
}

// SmartTruncate shortens the text on a word boundary when the MaxLength option
// is used with smart truncation enabled.
func (lengthGuardStage) SmartTruncate(s string) string {
	if MaxLength > 0 && EnableSmartTruncate {
		return smartTruncate(s)
	}
	return s
}

// sanitizingStage owns the phases that shape the separator characters of the
// finished slug.
type sanitizingStage struct{ noopStage }

func (sanitizingStage) StageName() string { return "sanitizing" }

// ReplaceUnauthorized processes all remaining symbols.
func (sanitizingStage) ReplaceUnauthorized(s string) string {
	return regexpNonAuthorizedChars.ReplaceAllString(s, "-")
}

// CollapseDashes trims multiple dashes unless the DisableMultipleDashTrim
// option says otherwise.
func (sanitizingStage) CollapseDashes(s string) string {
	if !DisableMultipleDashTrim {
		return regexpMultipleDashes.ReplaceAllString(s, "-")
	}
	return s
}

// TrimEdgeChars keeps or removes the leading and trailing dashes and
// underscores, following the DisableEndsTrim option.
func (sanitizingStage) TrimEdgeChars(s string) string {
	if !DisableEndsTrim {
		return strings.Trim(s, "-_")
	}
	return s
}

// uniquifyingStage appends the timestamp requested by the AppendTimestamp
// option.
type uniquifyingStage struct{ noopStage }

func (uniquifyingStage) StageName() string { return "uniquifying" }

// AppendUniqueSuffix appends a timestamp to the end in order to make slug
// unique.
func (uniquifyingStage) AppendUniqueSuffix(s string) string {
	if AppendTimestamp {
		return s + "-" + timestamp()
	}
	return s
}
