import React from 'react'

type NativeVideoProps = React.HTMLProps<HTMLVideoElement>
type VideoProps = NativeVideoProps & {
  paused: boolean
  currentQuality: string
  useAutoQuality: boolean
  sourceQualities: string[]
  playUrls: string[]
  sourceWidths: number[]
  sourceHeights: number[]
  sourceBitrates: number[]
  onRef: (el: HTMLVideoElement | null) => void
}

const NormalVideo: React.FC<VideoProps> = (props) => {
  const {
    onRef,
    /* eslint-disable @typescript-eslint/no-unused-vars */
    paused,
    currentQuality,
    useAutoQuality,
    sourceQualities,
    playUrls,
    sourceWidths,
    sourceHeights,
    sourceBitrates,
    /* eslint-disable @typescript-eslint/no-unused-vars */
    ...restProps
  } = props
  return <video {...restProps} ref={onRef} />
}

export default {
  pluginName: 'native',
  VideoComponent: NormalVideo,
  willHandleSrcChange: false,
}
