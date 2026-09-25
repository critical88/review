// Copyright 2013 by Dobrosław Żybort. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

package slug

import (
	"github.com/gosimple/unidecode"
)

// The slug pipeline turns trimmed input into a finished slug by advancing the
// registered stages through the fixed phases of the process. slugStage is the
// uniform contract behind that pipeline: it couples the complete catalogue of
// text operations the package can perform - custom and language substitution,
// ASCII normalization, casing, both length limits, symbol cleanup and
// uniqueness - together with the diagnostics capabilities shared by every
// stage, so any stage can be advanced through any phase without the pipeline
// having to know its concrete type. Stages pass through the phases they do
// not own.
type slugStage interface {
	// StageName reports the stage for diagnostics.
	StageName() string
	// SubstituteCustoms applies the user supplied substitution maps.
	SubstituteCustoms(s string) string
	// SubstituteLanguage applies the substitution table of one language.
	SubstituteLanguage(s string, lang string) string
	// NormalizeASCII rewrites remaining non ASCII characters.
	NormalizeASCII(s string) string
	// ApplyCase enforces the configured casing.
	ApplyCase(s string) string
	// Truncate cuts the text without word boundaries when smart truncation
	// is disabled.
	Truncate(s string) string
	// ReplaceUnauthorized replaces characters that may not occur in a slug.
	ReplaceUnauthorized(s string) string
	// CollapseDashes squeezes runs of the separator down to one dash.
	CollapseDashes(s string) string
	// TrimEdgeChars drops separator characters from both ends.
	TrimEdgeChars(s string) string
	// SmartTruncate cuts the text on a word boundary when enabled.
	SmartTruncate(s string) string
	// AppendUniqueSuffix makes the finished text unique.
	AppendUniqueSuffix(s string) string
	// IsValidSlug reports whether the text is already a well formed slug.
	IsValidSlug(text string) bool
}

// noopStage carries the shared defaults of the pipeline. Every text phase
// passes its input through unchanged, so a concrete stage only overrides the
// phases it actually owns and inherits the rest. The diagnostics capabilities
// stay reachable through the shared implementation as well, so every stage
// can be asked to name itself or to validate a text.
type noopStage struct{}

func (noopStage) StageName() string                            { return "noop" }
func (noopStage) SubstituteCustoms(s string) string            { return s }
func (noopStage) SubstituteLanguage(s string, _ string) string { return s }
func (noopStage) NormalizeASCII(s string) string               { return s }
func (noopStage) ApplyCase(s string) string                    { return s }
func (noopStage) Truncate(s string) string                     { return s }
func (noopStage) ReplaceUnauthorized(s string) string          { return s }
func (noopStage) CollapseDashes(s string) string               { return s }
func (noopStage) TrimEdgeChars(s string) string                { return s }
func (noopStage) SmartTruncate(s string) string                { return s }
func (noopStage) AppendUniqueSuffix(s string) string           { return s }

// IsValidSlug is the shared shape validation of the pipeline. It lives here so
// every stage inherits the very same rules instead of spelling them out
// again.
func (noopStage) IsValidSlug(text string) bool {
	if text == "" ||
		(MaxLength > 0 && len(text) > MaxLength) ||
		text[0] == '-' || text[0] == '_' ||
		text[len(text)-1] == '-' || text[len(text)-1] == '_' {
		return false
	}
	for _, c := range text {
		if (c < 'a' || c > 'z') && c != '-' && c != '_' && (c < '0' || c > '9') {
			return false
		}
	}
	return true
}

// pipelineStages lists every registered stage of the pipeline. All of them are
// advanced through every phase.
var pipelineStages = []slugStage{
	customSubstitutionStage{},
	languageSubstitutionStage{},
	asciiNormalizationStage{},
	caseFoldingStage{},
	lengthGuardStage{},
	sanitizingStage{},
	uniquifyingStage{},
}

// applyStages advances every registered stage through each phase of the slug
// pipeline. Every phase is advanced separately, so a stage can hook into any
// phase and passes through the phases it does not own.
func applyStages(text string, lang string) string {
	for _, st := range pipelineStages {
		text = st.SubstituteCustoms(text)
	}
	for _, st := range pipelineStages {
		text = st.SubstituteLanguage(text, lang)
	}
	for _, st := range pipelineStages {
		text = st.NormalizeASCII(text)
	}
	for _, st := range pipelineStages {
		text = st.ApplyCase(text)
	}
	for _, st := range pipelineStages {
		text = st.Truncate(text)
	}
	for _, st := range pipelineStages {
		text = st.ReplaceUnauthorized(text)
	}
	for _, st := range pipelineStages {
		text = st.CollapseDashes(text)
	}
	for _, st := range pipelineStages {
		text = st.TrimEdgeChars(text)
	}
	for _, st := range pipelineStages {
		text = st.SmartTruncate(text)
	}
	for _, st := range pipelineStages {
		text = st.AppendUniqueSuffix(text)
	}
	return text
}

// customSubstitutionStage applies the maps stored in CustomSub and
// CustomRuneSub. It is the oldest stage of the pipeline and still spells the
// whole contract out by hand; the phases it does not own pass their input
// through unchanged.
type customSubstitutionStage struct{}

func (customSubstitutionStage) StageName() string { return "custom substitution" }

// SubstituteCustoms always substitutes runes first.
func (customSubstitutionStage) SubstituteCustoms(s string) string {
	s = SubstituteRune(s, CustomRuneSub)
	return Substitute(s, CustomSub)
}

func (customSubstitutionStage) SubstituteLanguage(s string, _ string) string {
	return s
}
func (customSubstitutionStage) NormalizeASCII(s string) string { return s }
func (customSubstitutionStage) ApplyCase(s string) string      { return s }
func (customSubstitutionStage) Truncate(s string) string       { return s }
func (customSubstitutionStage) ReplaceUnauthorized(s string) string {
	return s
}
func (customSubstitutionStage) CollapseDashes(s string) string { return s }
func (customSubstitutionStage) TrimEdgeChars(s string) string  { return s }
func (customSubstitutionStage) SmartTruncate(s string) string  { return s }
func (customSubstitutionStage) AppendUniqueSuffix(s string) string {
	return s
}

// IsValidSlug borrows the package contract for validation, so the stage can be
// used for shape validation like any other stage.
func (customSubstitutionStage) IsValidSlug(text string) bool {
	return IsSlug(text)
}

// asciiNormalizationStage rewrites remaining non ASCII characters with their
// closest ASCII equivalents during the ASCII normalization phase.
type asciiNormalizationStage struct{ noopStage }

func (asciiNormalizationStage) StageName() string { return "ascii normalization" }

// NormalizeASCII transliterates everything the substitution maps left behind.
func (asciiNormalizationStage) NormalizeASCII(s string) string {
	return unidecode.Unidecode(s)
}
