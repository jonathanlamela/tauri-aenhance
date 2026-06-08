import { describe, it, expect } from 'vitest'
import { formatDuration, buildSuggestedOutputPath } from './utils.js'

describe('formatDuration', () => {
    it('formats zero seconds', () => {
        expect(formatDuration(0)).toBe('0:00')
    })

    it('formats sub-minute durations', () => {
        expect(formatDuration(59)).toBe('0:59')
    })

    it('formats exactly one minute', () => {
        expect(formatDuration(60)).toBe('1:00')
    })

    it('formats minutes and seconds', () => {
        expect(formatDuration(125)).toBe('2:05')
    })

    it('rounds fractional seconds', () => {
        expect(formatDuration(61.7)).toBe('1:02')
    })

    it('handles null/undefined gracefully', () => {
        expect(formatDuration(null)).toBe('0:00')
        expect(formatDuration(undefined)).toBe('0:00')
    })

    it('clamps negative values to zero', () => {
        expect(formatDuration(-5)).toBe('0:00')
    })
})

describe('buildSuggestedOutputPath', () => {
    it('replaces extension with -cleaned.wav for wav format', () => {
        expect(buildSuggestedOutputPath('/home/user/audio.mp3', 'wav'))
            .toBe('/home/user/audio-cleaned.wav')
    })

    it('replaces extension with -cleaned.mp3 for mp3 format', () => {
        expect(buildSuggestedOutputPath('/home/user/audio.wav', 'mp3'))
            .toBe('/home/user/audio-cleaned.mp3')
    })

    it('handles OGG input', () => {
        expect(buildSuggestedOutputPath('/recordings/session.ogg', 'wav'))
            .toBe('/recordings/session-cleaned.wav')
    })

    it('handles paths with spaces', () => {
        expect(buildSuggestedOutputPath('/my files/rec 01.wav', 'wav'))
            .toBe('/my files/rec 01-cleaned.wav')
    })

    it('replaces only the last extension', () => {
        expect(buildSuggestedOutputPath('/path/to/my.audio.file.wav', 'wav'))
            .toBe('/path/to/my.audio.file-cleaned.wav')
    })

    it('handles Windows-style backslash paths', () => {
        expect(buildSuggestedOutputPath('C:\\Users\\user\\audio.wav', 'mp3'))
            .toBe('C:\\Users\\user\\audio-cleaned.mp3')
    })
})
