import createMasterM3U8 from './createMasterM3U8'

export default function getMasterM3U8Blob(
  playUrls: string[],
  sourceBitrates: number[] = [],
  sourceWidths: number[] = [],
  sourceHeights: number[] = []
): Blob {
  const list = playUrls.map((source, index) => ({
    source,
    bandwidth: sourceBitrates[index] * 1024,

    resolution: {
      width: sourceWidths[index],
      height: sourceHeights[index],
    },
  }))

  return new Blob([createMasterM3U8(list)], {
    type: 'application/vnd.apple.mpegURL',
  })
}
