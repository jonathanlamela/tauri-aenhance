export function formatDuration(seconds) {
    const totalSeconds = Math.max(0, Math.round(seconds || 0))
    const m = Math.floor(totalSeconds / 60)
    const r = totalSeconds % 60
    return `${m}:${String(r).padStart(2, '0')}`
}

export function buildSuggestedOutputPath(currentInputPath, format) {
    const ext = format === 'mp3' ? 'mp3' : 'wav'
    return currentInputPath.replace(/\.[^.]+$/, `-cleaned.${ext}`)
}
