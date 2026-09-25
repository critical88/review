package pinyin

import (
	"regexp"
)

// 带音标字符。
var phoneticSymbol = map[string]string{
	"ā": "a1",
	"á": "a2",
	"ǎ": "a3",
	"à": "a4",
	"ē": "e1",
	"é": "e2",
	"ě": "e3",
	"è": "e4",
	"ō": "o1",
	"ó": "o2",
	"ǒ": "o3",
	"ò": "o4",
	"ī": "i1",
	"í": "i2",
	"ǐ": "i3",
	"ì": "i4",
	"ū": "u1",
	"ú": "u2",
	"ǔ": "u3",
	"ù": "u4",
	"ü": "v",
	"ǘ": "v2",
	"ǚ": "v3",
	"ǜ": "v4",
	"ń": "n2",
	"ň": "n3",
	"ǹ": "n4",
	"ḿ": "m2",
}

// 所有带声调的字符
var rePhoneticSymbolSource = func(m map[string]string) string {
	s := ""
	for k := range m {
		s = s + k
	}
	return s
}(phoneticSymbol)

// 匹配带声调字符的正则表达式
var rePhoneticSymbol = regexp.MustCompile("[" + rePhoneticSymbolSource + "]")

// 匹配使用数字标识声调的字符的正则表达式
var reTone2 = regexp.MustCompile("([aeoiuvnm])([1-4])$")

// 按照目标风格替换拼音中的带声调字符
func decodePhoneticSymbol(p string, a Args) string {
	return rePhoneticSymbol.ReplaceAllStringFunc(p, func(m string) string {
		symbol, _ := phoneticSymbol[m]
		switch a.Style {
		// 不包含声调
		case Normal, FirstLetter, Finals:
			// 去掉声调: a1 -> a
			m = reTone2.ReplaceAllString(symbol, "$1")
		case Tone2, FinalsTone2, Tone3, FinalsTone3:
			// 返回使用数字标识声调的字符
			m = symbol
		default:
			// 声调在头上
		}
		return m
	})
}
