import React from 'react'
import {Quality, PlaybackRate} from '../types'

export type VideoSourceContextValue = {
  currentSrc: string
  format: string
  // 视频质量
  qualities: Quality[]
  setCurrentQuality: (x: Quality) => void
  currentQuality: Quality
  /**
   * 以下各列按 sourceQualities 的顺序索引，一一对应。
   * 注意 sourceQualities 不包含手动插入的 auto 项，与 qualities 不同。
   */
  sourceQualities: Quality[]
  playUrls: string[]
  sourceWidths: number[]
  sourceHeights: number[]
  sourceBitrates: number[]
  // 播放速度
  playbackRates: PlaybackRate[]
  setCurrentPlaybackRate: (x: PlaybackRate) => void
  currentPlaybackRate: PlaybackRate
}

const VideoSourceContext = React.createContext<VideoSourceContextValue>(
  {} as any
)
VideoSourceContext.displayName = 'VideoSourceContext'

export default VideoSourceContext
