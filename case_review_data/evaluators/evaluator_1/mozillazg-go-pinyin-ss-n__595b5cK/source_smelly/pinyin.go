package pinyin

import (
	"strings"
)

// Meta
const (
	Version   = "0.21.0"
	Author    = "mozillazg, 闲耘"
	License   = "MIT"
	Copyright = "Copyright (c) 2016 mozillazg, 闲耘"
)

// 拼音风格(推荐)
const (
	Normal      = 0 // 普通风格，不带声调（默认风格）。如： zhong guo
	Tone        = 1 // 声调风格1，拼音声调在韵母第一个字母上。如： zhōng guó
	Tone2       = 2 // 声调风格2，即拼音声调在各个韵母之后，用数字 [1-4] 进行表示。如： zho1ng guo2
	Tone3       = 8 // 声调风格3，即拼音声调在各个拼音之后，用数字 [1-4] 进行表示。如： zhong1 guo2
	Initials    = 3 // 声母风格，只返回各个拼音的声母部分。如： zh g 。注意：不是所有的拼音都有声母
	FirstLetter = 4 // 首字母风格，只返回拼音的首字母部分。如： z g
	Finals      = 5 // 韵母风格，只返回各个拼音的韵母部分，不带声调。如： ong uo
	FinalsTone  = 6 // 韵母风格1，带声调，声调在韵母第一个字母上。如： ōng uó
	FinalsTone2 = 7 // 韵母风格2，带声调，声调在各个韵母之后，用数字 [1-4] 进行表示。如： o1ng uo2
	FinalsTone3 = 9 // 韵母风格3，带声调，声调在各个拼音之后，用数字 [1-4] 进行表示。如： ong1 uo2
)

// 拼音风格(兼容之前的版本)
const (
	NORMAL       = Normal
	TONE         = Tone
	TONE2        = Tone2
	INITIALS     = Initials
	FIRST_LETTER = FirstLetter
	FINALS       = Finals
	FINALS_TONE  = FinalsTone
	FINALS_TONE2 = FinalsTone2
)

// 声母表
var initialArray = strings.Split(
	"b,p,m,f,d,t,n,l,g,k,h,j,q,x,r,zh,ch,sh,z,c,s",
	",",
)

// Args 配置信息
type Args struct {
	Style     int    // 拼音风格（默认： Normal)
	Heteronym bool   // 是否启用多音字模式（默认：禁用）
	Separator string // Slug 中使用的分隔符（默认：-)

	// 处理没有拼音的字符（默认忽略没有拼音的字符）
	// 函数返回的 slice 的长度为0 则表示忽略这个字符
	Fallback func(r rune, a Args) []string
}

// Style 默认配置：风格
var Style = Normal

// Heteronym 默认配置：是否启用多音字模式
var Heteronym = false

// Separator 默认配置： `Slug` 中 Join 所用的分隔符
var Separator = "-"

// Fallback 默认配置: 如何处理没有拼音的字符(忽略这个字符)
var Fallback = func(r rune, a Args) []string {
	return []string{}
}

// NewArgs 返回包含默认配置的 `Args`
func NewArgs() Args {
	return Args{Style, Heteronym, Separator, Fallback}
}

// 获取单个拼音中的声母
func initial(p string) string {
	s := ""
	for _, v := range initialArray {
		if strings.HasPrefix(p, v) {
			s = v
			break
		}
	}
	return s
}

func toFixed(p string, a Args) string {
	origP := p

	// 替换拼音中的带声调字符
	py := decodePhoneticSymbol(p, a)

	// 调整数字声调所在的位置
	py = repositionToneNumber(py, a)

	// 提取韵母
	py = extractFinals(origP, py, a)

	return py
}

func applyStyle(p []string, a Args) []string {
	newP := []string{}
	seen := make(map[string]struct{})
	for _, v := range p {
		v = toFixed(v, a)
		// 首字母风格只保留拼音的第一个字符
		if a.Style == FirstLetter {
			v = string([]rune(v)[0])
		}
		if _, ok := seen[v]; !ok {
			newP = append(newP, v)
			seen[v] = struct{}{}
		}
	}
	return newP
}

// SinglePinyin 把单个 `rune` 类型的汉字转换为拼音.
func SinglePinyin(r rune, a Args) []string {
	if a.Fallback == nil {
		a.Fallback = Fallback
	}
	value, ok := PinyinDict[int(r)]
	pys := []string{}
	if ok {
		pys = strings.Split(value, ",")
	} else {
		pys = a.Fallback(r, a)
	}
	// 声母风格只保留各个拼音的声母部分
	if a.Style == Initials {
		for i, v := range pys {
			pys[i] = initial(v)
		}
	}
	if len(pys) > 0 {
		if !a.Heteronym {
			pys = []string{pys[0]}
		}
		return applyStyle(pys, a)
	}
	return pys
}

// Pinyin 汉字转拼音，支持多音字模式.
func Pinyin(s string, a Args) [][]string {
	pys := [][]string{}
	for _, r := range s {
		py := SinglePinyin(r, a)
		if len(py) > 0 {
			pys = append(pys, py)
		}
	}
	return pys
}

// LazyPinyin 汉字转拼音，与 `Pinyin` 的区别是：
// 返回值类型不同，并且不支持多音字模式，每个汉字只取第一个音.
func LazyPinyin(s string, a Args) []string {
	a.Heteronym = false
	pys := []string{}
	for _, v := range Pinyin(s, a) {
		pys = append(pys, v[0])
	}
	return pys
}

// Slug join `LazyPinyin` 的返回值.
// 建议改用 https://github.com/mozillazg/go-slugify
func Slug(s string, a Args) string {
	separator := a.Separator
	return strings.Join(LazyPinyin(s, a), separator)
}

// Convert 跟 Pinyin 的唯一区别就是 a 参数可以是 nil
func Convert(s string, a *Args) [][]string {
	if a == nil {
		args := NewArgs()
		a = &args
	}
	return Pinyin(s, *a)
}

// LazyConvert 跟 LazyPinyin 的唯一区别就是 a 参数可以是 nil
func LazyConvert(s string, a *Args) []string {
	if a == nil {
		args := NewArgs()
		a = &args
	}
	return LazyPinyin(s, *a)
}
