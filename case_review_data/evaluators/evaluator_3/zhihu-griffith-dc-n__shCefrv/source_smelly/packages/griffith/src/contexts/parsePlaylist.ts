import {Quality, PlaySourceMap} from '../types'
const QUALITY_ORDER: Quality[] = ['auto', 'ld', 'sd', 'hd', 'fhd']

export const getQualities = (
  sources: PlaySourceMap,
  isMobile: any,
  isDescOrder: boolean
) => {
  const qualities = (Object.keys(sources) as Quality[]).sort((a, b) =>
    isDescOrder
      ? QUALITY_ORDER.indexOf(b) - QUALITY_ORDER.indexOf(a)
      : QUALITY_ORDER.indexOf(a) - QUALITY_ORDER.indexOf(b)
  )

  if (qualities.length > 1) {
    if (isMobile) {
      // 移动端只返回最低清晰度
      return qualities.slice(0, 1)
    } else {
      // 桌面端端去掉低清，除非只有一个低清
      return qualities.filter((item) => item !== 'ld')
    }
  }

  return qualities
}

// 以下 getter 均按 qualities 的顺序返回各列，避免为整个播放列表构建中间对象

export const getPlayUrls = (qualities: Quality[], sources: PlaySourceMap) =>
  qualities.map((quality) =>
    // @ts-expect-error Property 'auto' does not exist on type 'PlaySourceMap'，应当是 QUALITY_ORDER 定义错了
    sources[quality].play_url
  ) as string[]

export const getSourceHeights = (qualities: Quality[], sources: PlaySourceMap) =>
  qualities.map(
    (quality) =>
      // @ts-expect-error Property 'auto' does not exist on type 'PlaySourceMap'，应当是 QUALITY_ORDER 定义错了
      sources[quality].height
  ) as number[]

export const getSourceWidths = (qualities: Quality[], sources: PlaySourceMap) =>
  qualities.map(
    (quality) =>
      // @ts-expect-error Property 'auto' does not exist on type 'PlaySourceMap'，应当是 QUALITY_ORDER 定义错了
      sources[quality].width
  ) as number[]

export const getSourceBitrates = (
  qualities: Quality[],
  sources: PlaySourceMap
) =>
  qualities.map(
    (quality) =>
      // @ts-expect-error Property 'auto' does not exist on type 'PlaySourceMap'，应当是 QUALITY_ORDER 定义错了
      sources[quality].bitrate
  ) as number[]
