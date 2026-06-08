<script setup>
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open, save } from '@tauri-apps/plugin-dialog'
import { useI18n } from 'vue-i18n'
import WaveformPlayer from './components/WaveformPlayer.vue'
import { formatDuration, buildSuggestedOutputPath } from './utils.js'

const { locale, t } = useI18n()

const inputPath = ref('')
const outputPath = ref('')
const processedPath = ref('')
const outputFormat = ref('wav')
const normalize = ref(false)
const volumeBoostDb = ref(0)
const boosting = ref(false)
const metadata = ref(null)
const busy = ref(false)
const analyzing = ref(false)
const currentTask = ref('')
const progressValue = ref(0)
const progressStage = ref('')
const statusMessage = ref('')
const errorMessage = ref('')
let unlistenProgress = null

const languageOptions = [
    { value: 'en', labelKey: 'language.en' },
    { value: 'it', labelKey: 'language.it' },
]


const sourceFileName = computed(() => inputPath.value.split(/[\\/]/).pop() || '')
const isWorking = computed(() => busy.value || analyzing.value || boosting.value)
const canProcess = computed(() => Boolean(inputPath.value && outputPath.value && !isWorking.value))

const stageToKey = {
    preparing: 'progress.preparing',
    analyzing: 'progress.analyzing',
    decoding: 'progress.decoding',
    resampling: 'progress.resampling',
    denoising: 'progress.denoising',
    boosting: 'progress.boosting',
    processing: 'progress.processing',
    writing: 'progress.writing',
    transcoding: 'progress.transcoding',
    done: 'progress.done',
}

const stageLog = ref([])
const stageOrder = ['decoding', 'resampling', 'denoising', 'boosting', 'writing', 'transcoding', 'done']

const progressLabel = computed(() => {
    const key = stageToKey[progressStage.value] || 'progress.preparing'
    return t(key)
})

function channelLabel(channels) {
    if (channels === 1) return t('status.mono')
    if (channels === 2) return t('status.stereo')
    return `${channels}ch`
}

async function pickInputFile() {
    errorMessage.value = ''
    statusMessage.value = ''

    const selected = await open({
        multiple: false,
        directory: false,
        filters: [{ name: 'Audio', extensions: ['mp3', 'wav', 'ogg'] }],
    })

    if (typeof selected !== 'string') return

    inputPath.value = selected
    processedPath.value = ''
    metadata.value = null
    outputPath.value = buildSuggestedOutputPath(selected, outputFormat.value)
    analyzing.value = true
    currentTask.value = 'analyze'
    progressStage.value = 'preparing'

    try {
        metadata.value = await invoke('analyze_audio', { request: { path: selected } })
    } catch (error) {
        errorMessage.value = error?.message || t('feedback.analyzeError')
    } finally {
        analyzing.value = false
        currentTask.value = ''
    }
}

async function pickOutputPath() {
    errorMessage.value = ''
    if (!inputPath.value) { errorMessage.value = t('file.missingInput'); return }

    const selected = await save({
        defaultPath: outputPath.value || buildSuggestedOutputPath(inputPath.value, outputFormat.value),
        filters: [{ name: outputFormat.value.toUpperCase(), extensions: [outputFormat.value] }],
    })

    if (typeof selected === 'string') outputPath.value = selected
}

async function processAudio() {
    errorMessage.value = ''
    statusMessage.value = ''

    if (!inputPath.value) { errorMessage.value = t('file.missingInput'); return }
    if (!outputPath.value) { errorMessage.value = t('file.missingOutput'); return }

    busy.value = true
    currentTask.value = 'process'
    progressStage.value = 'decoding'
    progressValue.value = 5
    stageLog.value = []

    try {
        const result = await invoke('process_audio', {
            request: {
                inputPath: inputPath.value,
                outputPath: outputPath.value,
                outputFormat: outputFormat.value,
                normalize: normalize.value,
            },
        })
        processedPath.value = result.outputPath
        statusMessage.value = t('feedback.outputReady')
    } catch (error) {
        const msg = error?.message || ''
        if (msg === 'cancelled') {
            statusMessage.value = t('feedback.cancelled')
        } else {
            errorMessage.value = msg || t('feedback.processError')
        }
    } finally {
        busy.value = false
        currentTask.value = ''
        progressStage.value = 'done'
        progressValue.value = 100
    }
}

async function stopProcess() {
    await invoke('cancel_process')
}

async function boostVolume() {
    if (!processedPath.value || volumeBoostDb.value === 0) return
    errorMessage.value = ''
    boosting.value = true
    try {
        await invoke('boost_volume', {
            path: processedPath.value,
            gainDb: volumeBoostDb.value,
        })
        // Force waveform reload by toggling the path
        const p = processedPath.value
        processedPath.value = ''
        await new Promise(r => setTimeout(r, 50))
        processedPath.value = p
        statusMessage.value = t('feedback.volumeApplied')
    } catch (error) {
        errorMessage.value = error?.message || t('feedback.processError')
    } finally {
        boosting.value = false
    }
}

onMounted(async () => {
    unlistenProgress = await listen('audio-progress', (event) => {
        const payload = event.payload || {}
        if (payload.task !== currentTask.value) return
        if (typeof payload.percent === 'number') {
            progressValue.value = Math.max(0, Math.min(100, payload.percent))
        }
        if (typeof payload.stage === 'string' && payload.stage !== progressStage.value) {
            // Append completed stage to log
            if (progressStage.value && progressStage.value !== 'done') {
                stageLog.value.push(progressStage.value)
            }
            progressStage.value = payload.stage
        }
    })
})

onBeforeUnmount(() => {
    if (unlistenProgress) { unlistenProgress(); unlistenProgress = null }
})

watch(outputFormat, (newFormat) => {
    if (inputPath.value) outputPath.value = buildSuggestedOutputPath(inputPath.value, newFormat)
})
</script>

<template>
    <main class="min-h-dvh bg-slate-50 text-slate-800">

        <div class="mx-auto flex min-h-dvh max-w-3xl flex-col gap-5 px-4 py-6 sm:px-6">

            <!-- Header — pt-7 accounts for the transparent native title bar (~28px) -->
            <header class="flex items-center justify-between pt-7">
                <h1 class="select-none text-lg font-semibold tracking-tight">AEnhance</h1>
                <select v-model="locale"
                    class="rounded-lg border border-slate-200 bg-white px-2 py-1.5 text-xs outline-none focus:border-blue-500">
                    <option v-for="option in languageOptions" :key="option.value" :value="option.value">
                        {{ t(option.labelKey) }}
                    </option>
                </select>
            </header>

            <!-- Main card -->
            <section class="rounded-2xl border border-slate-200 bg-white p-5 shadow-sm space-y-5">

                <!-- File picker row -->
                <div class="flex flex-col gap-3 sm:flex-row sm:items-start sm:gap-4">
                    <button
                        class="w-full rounded-xl bg-blue-600 px-4 py-2 text-sm font-medium text-white transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-50 sm:w-auto sm:shrink-0"
                        :disabled="isWorking" @click="pickInputFile">
                        {{ inputPath ? t('file.change') : t('file.select') }}
                    </button>

                    <div class="min-w-0 flex-1 sm:pt-1">
                        <div v-if="analyzing" class="flex items-center gap-2 text-sm text-slate-500">
                            <svg class="h-4 w-4 animate-spin text-blue-600" viewBox="0 0 24 24" fill="none">
                                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor"
                                    stroke-width="4" />
                                <path class="opacity-75" fill="currentColor"
                                    d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                            </svg>
                            {{ t('progress.analyzing') }}
                        </div>
                        <template v-else-if="inputPath">
                            <p class="truncate text-sm font-medium">{{ sourceFileName }}</p>
                            <p v-if="metadata" class="mt-0.5 text-xs text-slate-400">
                                {{ formatDuration(metadata.durationSeconds) }} ·
                                {{ (metadata.sampleRate / 1000).toFixed(1) }} kHz ·
                                {{ channelLabel(metadata.channels) }}
                            </p>
                        </template>
                        <p v-else class="text-sm text-slate-400">{{ t('app.noInput') }}</p>
                    </div>
                </div>

                <div class="h-px bg-slate-100"></div>

                <!-- Output row -->
                <div class="flex flex-col gap-2 sm:grid sm:grid-cols-[minmax(0,1fr)_auto_6rem]">
                    <input v-model="outputPath" readonly
                        class="h-10 w-full rounded-xl border border-slate-200 bg-slate-50 px-3 text-sm text-slate-600 outline-none disabled:opacity-50"
                        type="text" :placeholder="t('file.outputHint')" :disabled="isWorking" />
                    <div class="flex flex-col gap-2 sm:contents">
                        <button
                            class="h-10 w-full rounded-xl border border-slate-200 bg-white px-3 text-sm text-slate-600 transition hover:bg-slate-50 disabled:cursor-not-allowed disabled:opacity-50"
                            :disabled="isWorking" @click="pickOutputPath">
                            {{ t('file.saveTo') }}
                        </button>
                        <select v-model="outputFormat"
                            class="h-10 w-full rounded-xl border border-slate-200 bg-white px-3 text-sm outline-none focus:border-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
                            :disabled="isWorking">
                            <option value="wav">WAV</option>
                            <option value="mp3">MP3</option>
                        </select>
                    </div>
                </div>

                <!-- Normalize -->
                <label class="flex cursor-pointer items-center gap-3"
                    :class="{ 'opacity-50 pointer-events-none': isWorking }">
                    <input v-model="normalize" type="checkbox" class="h-4 w-4 accent-blue-600" :disabled="isWorking" />
                    <span class="text-sm text-slate-600">{{ t('controls.normalize') }}</span>
                </label>

                <!-- Progress bar + log -->
                <div v-if="busy" class="space-y-2">
                    <div class="flex justify-between text-xs text-slate-500">
                        <span class="flex items-center gap-1.5">
                            <svg class="h-3 w-3 animate-spin text-blue-600" viewBox="0 0 24 24" fill="none">
                                <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4" />
                                <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
                            </svg>
                            {{ progressLabel }}
                        </span>
                        <span>{{ progressValue }}%</span>
                    </div>
                    <div class="h-1.5 overflow-hidden rounded-full bg-slate-100">
                        <div class="h-full rounded-full bg-blue-600 transition-all duration-200"
                            :style="{ width: `${progressValue}%` }"></div>
                    </div>
                    <div v-if="stageLog.length > 0" class="space-y-0.5 pt-0.5">
                        <div v-for="stage in stageLog" :key="stage"
                            class="flex items-center gap-2 text-xs text-slate-400">
                            <svg class="h-3 w-3 shrink-0 text-green-500" viewBox="0 0 12 12" fill="currentColor">
                                <path d="M10 3L5 8.5 2 5.5" stroke="currentColor" stroke-width="1.5" fill="none" stroke-linecap="round" stroke-linejoin="round"/>
                            </svg>
                            {{ t(stageToKey[stage] || 'progress.preparing') }}
                        </div>
                    </div>
                </div>

                <!-- Process / Stop buttons -->
                <div class="flex gap-3">
                    <button v-if="!isWorking"
                        class="flex-1 rounded-xl bg-blue-600 py-2.5 text-sm font-semibold text-white transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-40"
                        :disabled="!canProcess" @click="processAudio">
                        {{ t('app.processing') }}
                    </button>
                    <template v-else>
                        <div
                            class="flex flex-1 items-center justify-center rounded-xl border border-slate-200 bg-slate-50 py-2.5 text-sm text-slate-400">
                            {{ t('app.processingBusy') }}
                        </div>
                        <button
                            class="rounded-xl bg-red-500 px-5 py-2.5 text-sm font-semibold text-white transition hover:bg-red-600"
                            @click="stopProcess">
                            {{ t('app.stop') }}
                        </button>
                    </template>
                </div>

                <!-- Feedback -->
                <div v-if="statusMessage"
                    class="rounded-xl border border-blue-100 bg-blue-50 px-3 py-2 text-sm text-blue-700">
                    {{ statusMessage }}
                </div>
                <div v-if="errorMessage"
                    class="rounded-xl border border-red-100 bg-red-50 px-3 py-2 text-sm text-red-600">
                    {{ errorMessage }}
                </div>

                <!-- Post-processing volume boost -->
                <div v-if="processedPath && !isWorking"
                    class="rounded-xl border border-slate-100 bg-slate-50 px-4 py-3">
                    <p class="mb-2 text-xs font-medium uppercase tracking-wide text-slate-400">
                        {{ t('controls.volumeSection') }}
                    </p>
                    <div class="flex items-center gap-3">
                        <input v-model.number="volumeBoostDb" type="range" min="0" max="20" step="1"
                            class="flex-1 accent-blue-600" />
                        <span class="w-14 text-right text-sm font-medium text-slate-700">
                            {{ volumeBoostDb > 0 ? `+${volumeBoostDb}` : '0' }} dB
                        </span>
                        <button
                            class="rounded-xl bg-blue-600 px-4 py-1.5 text-sm font-medium text-white transition hover:bg-blue-700 disabled:cursor-not-allowed disabled:opacity-40"
                            :disabled="volumeBoostDb === 0 || boosting" @click="boostVolume">
                            {{ boosting ? '...' : t('controls.applyVolume') }}
                        </button>
                    </div>
                </div>
            </section>

            <!-- Waveform players -->
            <section class="grid gap-4 sm:grid-cols-2">
                <WaveformPlayer :title="t('preview.original')" :file-path="inputPath" />
                <WaveformPlayer :title="t('preview.processed')" :file-path="processedPath" />
            </section>
        </div>
    </main>
</template>
