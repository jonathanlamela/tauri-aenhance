<script setup>
import { nextTick, onBeforeUnmount, ref, watch } from 'vue'
import { convertFileSrc } from '@tauri-apps/api/core'
import WaveSurfer from 'wavesurfer.js'
import { useI18n } from 'vue-i18n'

const props = defineProps({
    title: { type: String, required: true },
    filePath: { type: String, default: '' },
})

const { t } = useI18n()
const waveRoot = ref(null)
const isLoading = ref(false)
const errorMessage = ref('')
const isPlaying = ref(false)
const duration = ref(0)
const currentTime = ref(0)

let waveSurfer = null

function destroyWaveform() {
    if (waveSurfer) {
        waveSurfer.destroy()
        waveSurfer = null
    }
    isPlaying.value = false
    duration.value = 0
    currentTime.value = 0
}

async function loadWaveform(path) {
    destroyWaveform()
    errorMessage.value = ''

    if (!path) return

    isLoading.value = true
    await nextTick()

    if (!waveRoot.value) {
        isLoading.value = false
        return
    }

    try {
        const audioUrl = convertFileSrc(path)
        waveSurfer = WaveSurfer.create({
            container: waveRoot.value,
            waveColor: '#cbd5e1',
            progressColor: '#2563eb',
            cursorColor: '#93c5fd',
            barWidth: 2,
            barGap: 1,
            barRadius: 999,
            height: 72,
            normalize: true,
            hideScrollbar: true,
            url: audioUrl,
        })

        waveSurfer.on('ready', () => {
            duration.value = waveSurfer.getDuration()
            isLoading.value = false
        })

        waveSurfer.on('timeupdate', (time) => {
            currentTime.value = time
        })

        waveSurfer.on('finish', () => { isPlaying.value = false })
        waveSurfer.on('play', () => { isPlaying.value = true })
        waveSurfer.on('pause', () => { isPlaying.value = false })

        waveSurfer.on('error', () => {
            errorMessage.value = t('preview.playbackError')
            isLoading.value = false
        })
    } catch {
        errorMessage.value = t('preview.playbackError')
        isLoading.value = false
    }
}

function togglePlayback() {
    if (waveSurfer) waveSurfer.playPause()
}

function formatTime(seconds) {
    const s = Number.isFinite(seconds) ? seconds : 0
    const m = Math.floor(s / 60)
    const r = Math.floor(s % 60)
    return `${m}:${String(r).padStart(2, '0')}`
}

watch(() => props.filePath, (path) => loadWaveform(path), { immediate: true })

onBeforeUnmount(() => destroyWaveform())
</script>

<template>
    <section class="rounded-2xl border border-slate-200 bg-white p-4">
        <div class="mb-3 flex items-center justify-between">
            <p class="text-sm font-medium text-slate-700">{{ title }}</p>
            <button v-if="filePath && !errorMessage"
                class="flex h-7 w-7 items-center justify-center rounded-full bg-blue-600 text-white transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-40"
                :disabled="isLoading" @click="togglePlayback">
                <svg v-if="!isPlaying" class="h-3 w-3 translate-x-px" viewBox="0 0 10 12" fill="currentColor">
                    <path d="M0 0l10 6-10 6z" />
                </svg>
                <svg v-else class="h-3 w-3" viewBox="0 0 10 12" fill="currentColor">
                    <rect x="0" y="0" width="3.5" height="12" />
                    <rect x="6.5" y="0" width="3.5" height="12" />
                </svg>
            </button>
        </div>

        <div v-if="!filePath"
            class="flex h-20 items-center justify-center rounded-xl border border-dashed border-slate-200">
            <p class="text-xs text-slate-400">{{ t('preview.empty') }}</p>
        </div>

        <template v-else>
            <div v-if="errorMessage"
                class="flex h-20 items-center justify-center rounded-xl border border-red-100 bg-red-50">
                <p class="text-xs text-red-500">{{ errorMessage }}</p>
            </div>

            <div v-else class="relative rounded-xl border border-slate-100 bg-slate-50 px-3 py-3">
                <div v-if="isLoading"
                    class="absolute inset-0 z-10 flex items-center justify-center rounded-xl bg-white/80">
                    <svg class="h-5 w-5 animate-spin text-blue-600" viewBox="0 0 24 24" fill="none">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                        <path class="opacity-75" fill="currentColor"
                            d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                    </svg>
                </div>
                <div ref="waveRoot" :class="{ 'opacity-0': isLoading }"></div>
            </div>

            <div v-if="!errorMessage" class="mt-2 flex justify-between text-xs text-slate-400">
                <span>{{ formatTime(currentTime) }}</span>
                <span>{{ formatTime(duration) }}</span>
            </div>
        </template>
    </section>
</template>
