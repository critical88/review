import React, {useContext, useEffect, useMemo, useState} from 'react'
import {
  PlaySourceMap,
  PlaybackRate,
  RealQuality,
  Quality,
  QualityOrder,
} from '../types'
import VideoSourceContext from './VideoSourceContext'
import {
  getQualities,
  getPlayUrls,
  getSourceBitrates,
  getSourceHeights,
  getSourceWidths,
} from './parsePlaylist'
import {EVENTS} from 'griffith-message'
import {ua} from 'griffith-utils'
import reverseArray from '../utils/reverseArray'
import useHandler from '../hooks/useHandler'
import useChanged from '../hooks/useChanged'
import {InternalMessageContext} from './MessageContext'
import usePrevious from '../hooks/usePrevious'

const {isMobile} = ua

type VideoSourceProviderProps = {
  sources: PlaySourceMap
  defaultQuality?: RealQuality
  defaultQualityOrder?: QualityOrder
  useAutoQuality?: boolean
  playbackRates: PlaybackRate[]
  defaultPlaybackRate?: PlaybackRate
}

const VideoSourceProvider: React.FC<VideoSourceProviderProps> = ({
  sources: sourceMap,
  useAutoQuality,
  playbackRates,
  defaultPlaybackRate,
  defaultQuality,
  defaultQualityOrder,
  children,
}) => {
  const {emitEvent} = useContext(InternalMessageContext)
  const lastSourceMap = useChanged(sourceMap)
  const {
    qualities,
    sourceQualities,
    playUrls,
    sourceWidths,
    sourceHeights,
    sourceBitrates,
    format,
    isDescOrder,
  } = useMemo(() => {
    // 其实视频源应当是必需参数
    if (!lastSourceMap) {
      return {
        qualities: [] as Quality[],
        sourceQualities: [] as Quality[],
        playUrls: [] as string[],
        sourceWidths: [] as number[],
        sourceHeights: [] as number[],
        sourceBitrates: [] as number[],
      }
    }
    const {format} = Object.values(lastSourceMap)[0]!
    const isDescOrder = defaultQualityOrder === 'desc'
    const qualities = getQualities(lastSourceMap, isMobile, isDescOrder)
    // 列必须与 sourceQualities 对齐：不包含下面手动插入的 auto 项
    const sourceQualities = [...qualities]
    const playUrls = getPlayUrls(sourceQualities, lastSourceMap)
    const sourceWidths = getSourceWidths(sourceQualities, lastSourceMap)
    const sourceHeights = getSourceHeights(sourceQualities, lastSourceMap)
    const sourceBitrates = getSourceBitrates(sourceQualities, lastSourceMap)

    // 目前只有直播流实现了手动拼接 auto 清晰度的功能
    if (
      useAutoQuality &&
      !isMobile &&
      format === 'm3u8' &&
      !qualities.includes('auto')
    ) {
      qualities.unshift('auto')
    }

    return {
      qualities,
      sourceQualities,
      playUrls,
      sourceWidths,
      sourceHeights,
      sourceBitrates,
      format,
      isDescOrder,
    }
  }, [useAutoQuality, lastSourceMap, defaultQualityOrder])

  const [currentQuality, setCurrentQualityRaw] = useState(
    defaultQuality && (qualities as Quality[]).includes(defaultQuality)
      ? defaultQuality
      : qualities[0]
  )
  const [playbackRate, setPlaybackRate] = useState(defaultPlaybackRate)

  const setCurrentQuality = useHandler((quality: Quality) => {
    if (currentQuality !== quality) {
      setCurrentQualityRaw(quality)
      emitEvent(EVENTS.QUALITY_CHANGE, {
        prevQuality: currentQuality,
        quality,
      })
    }
  })

  // 当 sources 改变重置 `currentQuality`
  const prevQualities = usePrevious(qualities)
  useEffect(() => {
    if (prevQualities && prevQualities !== qualities) {
      setCurrentQuality(defaultQuality || qualities[0])
    }
  }, [prevQualities, qualities, defaultQuality, setCurrentQuality])

  const setCurrentPlaybackRate = useHandler((rate: PlaybackRate) => {
    if (playbackRate !== rate) {
      setPlaybackRate(rate)
      emitEvent(EVENTS.PLAYBACK_RATE_CHANGE, {
        prevRate: playbackRate!,
        rate,
      })
    }
  })

  // 根据当前清晰度返回对应的 src
  const currentSrc = useMemo(() => {
    if (playUrls.length === 0) {
      return
    }

    const index = sourceQualities.indexOf(currentQuality)
    return playUrls[index >= 0 ? index : 0]
  }, [currentQuality, playUrls, sourceQualities])

  const contextValue = useMemo(
    () => ({
      qualities: isDescOrder ? qualities : reverseArray(qualities),
      playbackRates,
      format: format!,
      sourceQualities,
      playUrls,
      sourceWidths,
      sourceHeights,
      sourceBitrates,
      currentQuality,
      currentPlaybackRate: playbackRate!,
      currentSrc: currentSrc!,
      setCurrentQuality,
      setCurrentPlaybackRate,
    }),
    [
      playbackRate,
      currentQuality,
      currentSrc,
      format,
      playbackRates,
      qualities,
      isDescOrder,
      sourceQualities,
      playUrls,
      sourceWidths,
      sourceHeights,
      sourceBitrates,
      setCurrentQuality,
      setCurrentPlaybackRate,
    ]
  )

  return (
    <VideoSourceContext.Provider value={contextValue}>
      {children}
    </VideoSourceContext.Provider>
  )
}

export default VideoSourceProvider
