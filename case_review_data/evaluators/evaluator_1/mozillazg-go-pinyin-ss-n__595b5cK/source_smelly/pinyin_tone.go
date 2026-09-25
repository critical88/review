package pinyin

import "regexp"

// 匹配 Tone3 中标识韵母声调的正则表达式
var reTone3 = regexp.MustCompile("^([a-z]+)([1-4])([a-z]*)$")

// Tone3/FinalsTone3 风格下声调用数字标识在拼音的最后，
// 需要把位于拼音中间的声调数字移动到最后
func repositionToneNumber(p string, a Args) string {
	switch a.Style {
	// 将声调移动到最后
	case Tone3, FinalsTone3:
		p = reTone3.ReplaceAllString(p, "$1$3$2")
	}
	return p
}
