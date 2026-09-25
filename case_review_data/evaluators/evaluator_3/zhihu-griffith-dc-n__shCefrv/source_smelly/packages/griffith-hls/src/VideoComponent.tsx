import React, {Component} from 'react'
// TODO: 升级有类型的新版
import Hls from 'hls.js/dist/hls.light.min'
import {getMasterM3U8Blob} from './utils'

type NativeVideoProps = React.HTMLProps<HTMLVideoElement>
// 各列按 sourceQualities 的顺序索引，一一对应
type VideoProps = NativeVideoProps & {
  paused: boolean
  currentQuality: string
  useAutoQuality: boolean
  sourceQualities: string[]
  playUrls: string[]
  sourceWidths: number[]
  sourceHeights: number[]
  sourceBitrates: number[]
  onRef(el: HTMLVideoElement | null): void
}

export default class VideoComponent extends Component<VideoProps> {
  hls?: Hls
  src!: string
  video: HTMLVideoElement | null = null
  manuallyBuildAdaptiveM3U8Blob = false
  hasLoadStarted = false

  componentDidMount() {
    const {
      src,
      useAutoQuality,
      playUrls,
      sourceQualities,
      sourceWidths,
      sourceHeights,
      sourceBitrates,
    } = this.props
    this.hls = new Hls({autoStartLoad: false})
    this.hls.attachMedia(this.video!)

    const isAutoQualitySourceProvided = sourceQualities.indexOf('auto') >= 0

    // 启用自动质量但是又没有提供 auto 规格的 source，那么就尝试本地手动生成
    if (useAutoQuality && !isAutoQualitySourceProvided) {
      const master = getMasterM3U8Blob(
        playUrls,
        sourceBitrates,
        sourceWidths,
        sourceHeights
      )
      this.src = URL.createObjectURL(master)
      this.manuallyBuildAdaptiveM3U8Blob = true
    } else {
      this.src = src!
    }

    this.hls.loadSource(this.src)
  }

  componentDidUpdate(prevProps: VideoProps) {
    const {currentQuality, playUrls, sourceQualities, paused, src} = this.props

    if (!this.hls) {
      return
    }

    if (currentQuality !== prevProps.currentQuality || prevProps.src !== src) {
      // 切换清晰度
      const qualityIndex = sourceQualities.indexOf(currentQuality)
      const playUrl = qualityIndex >= 0 ? playUrls[qualityIndex] : undefined
      if (playUrl !== undefined) {
        if (this.manuallyBuildAdaptiveM3U8Blob) {
          const levels = this.hls.levels
          const level = levels.findIndex((l) =>
            l.url.includes(playUrl as any)
          )
          this.hls.nextLevel = level
        } else {
          // TODO: 没有在 hls 的 API 内部找到顺畅切换 source 的方法
          // 因此这里比较直接和生硬
          const currentTime = this.video!.currentTime
          this.hls.destroy()
          this.hls = new Hls({autoStartLoad: false})
          this.hls.attachMedia(this.video!)
          this.hls.loadSource(playUrl)
          this.video!.currentTime = currentTime
          this.hls.startLoad()
          if (!paused) {
            void this.video!.play()
          }
        }
      } else {
        // 一定意味着选择了手动生成的「auto」
        this.hls.nextLevel = -1
      }
    }

    // 切换播放状态
    if (!paused && prevProps.paused && !this.hasLoadStarted) {
      this.hls.startLoad()
      this.hasLoadStarted = true
    }
  }

  componentWillUnmount() {
    this.hls!.destroy()
    if (this.manuallyBuildAdaptiveM3U8Blob) {
      URL.revokeObjectURL(this.src)
    }
  }

  render() {
    const {
      onRef,
      /* eslint-disable @typescript-eslint/no-unused-vars */
      currentQuality,
      useAutoQuality,
      src,
      sourceQualities,
      playUrls,
      sourceWidths,
      sourceHeights,
      sourceBitrates,
      paused,
      /* eslint-enable @typescript-eslint/no-unused-vars */
      ...props
    } = this.props
    return (
      <video
        ref={(el) => {
          if (onRef) {
            onRef(el)
          }
          this.video = el
        }}
        {...props}
      />
    )
  }
}
