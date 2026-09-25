package pinyin

import (
	"regexp"
	"strings"
)

var finalExceptionsMap = map[string]string{
	"ū": "ǖ",
	"ú": "ǘ",
	"ǔ": "ǚ",
	"ù": "ǜ",
}
var reFinalExceptions = regexp.MustCompile("^(j|q|x)(ū|ú|ǔ|ù)$")
var reFinal2Exceptions = regexp.MustCompile("^(j|q|x)u(\\d?)$")

// 获取单个拼音中的韵母
func final(p string) string {
	n := initial(p)
	if n == "" {
		return handleYW(p)
	}

	// 特例 j/q/x
	matches := reFinalExceptions.FindStringSubmatch(p)
	// jū -> jǖ
	if len(matches) == 3 && matches[1] != "" && matches[2] != "" {
		v, _ := finalExceptionsMap[matches[2]]
		return v
	}
	// ju -> jv, ju1 -> jv1
	p = reFinal2Exceptions.ReplaceAllString(p, "${1}v$2")
	return strings.Join(strings.SplitN(p, n, 2), "")
}

// 处理 y, w
func handleYW(p string) string {
	// 特例 y/w
	if strings.HasPrefix(p, "yu") {
		p = "v" + p[2:] // yu -> v
	} else if strings.HasPrefix(p, "yi") {
		p = p[1:] // yi -> i
	} else if strings.HasPrefix(p, "y") {
		p = "i" + p[1:] // y -> i
	} else if strings.HasPrefix(p, "wu") {
		p = p[1:] // wu -> u
	} else if strings.HasPrefix(p, "w") {
		p = "u" + p[1:] // w -> u
	}
	return p
}

// 韵母风格(Finals / FinalsTone / FinalsTone2 / FinalsTone3)只返回
// 各个拼音的韵母部分；鼻音字符(ḿ ń ň ǹ)没有声母所以不需要去掉声母部分
func extractFinals(origP string, p string, a Args) string {
	switch a.Style {
	// 韵母
	case Finals, FinalsTone, FinalsTone2, FinalsTone3:
		// 转换为 []rune unicode 编码用于获取第一个拼音字符
		// 因为 string 是 utf-8 编码不方便获取第一个拼音字符
		rs := []rune(origP)
		switch string(rs[0]) {
		// 因为鼻音没有声母所以不需要去掉声母部分
		case "ḿ", "ń", "ň", "ǹ":
		default:
			p = final(p)
		}
	}
	return p
}
