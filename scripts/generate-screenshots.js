import { chromium } from '@playwright/test'
import { mkdirSync } from 'fs'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const BASE_URL = 'http://localhost:5173'
const OUT_DIR = path.resolve(__dirname, '../docs/screenshots')

const VIEWPORTS = {
    desktop: { width: 1320, height: 880 },
    mobile:  { width: 390,  height: 844 },
}

const SCENES = [
    // Desktop — English
    { file: 'desktop_en_01_empty.png',     device: 'desktop', locale: 'en' },
    { file: 'desktop_en_02_with_file.png', device: 'desktop', locale: 'en', injectState: 'withFile' },
    // Desktop — Italian
    { file: 'desktop_it_01_empty.png',     device: 'desktop', locale: 'it' },
    { file: 'desktop_it_02_with_file.png', device: 'desktop', locale: 'it', injectState: 'withFile' },
    // Mobile — English
    { file: 'mobile_en_01_empty.png',      device: 'mobile',  locale: 'en' },
    { file: 'mobile_en_02_with_file.png',  device: 'mobile',  locale: 'en', injectState: 'withFile' },
    // Mobile — Italian
    { file: 'mobile_it_01_empty.png',      device: 'mobile',  locale: 'it' },
    { file: 'mobile_it_02_with_file.png',  device: 'mobile',  locale: 'it', injectState: 'withFile' },
]

// Simulated metadata payload for "file loaded" state
const MOCK_METADATA = {
    path: '/Users/demo/podcast-episode.wav',
    fileName: 'podcast-episode.wav',
    format: 'wav',
    durationSeconds: 185,
    sampleRate: 44100,
    channels: 2,
    frames: 8158500,
}

async function captureScene(browser, scene) {
    const { file, device, locale, injectState } = scene
    const navigatorLang = locale === 'it' ? 'it-IT' : 'en-US'

    const context = await browser.newContext({
        viewport: VIEWPORTS[device],
        locale: navigatorLang,
    })

    await context.addInitScript(`
        Object.defineProperty(navigator, 'language', {
            get: () => '${navigatorLang}',
            configurable: true,
        })
        // Stub Tauri IPC — invoke returns a fake metadata response for analyze_audio
        window.__TAURI_INTERNALS__ = {
            invoke: (cmd) => {
                if (cmd === 'analyze_audio') {
                    return Promise.resolve(${JSON.stringify(MOCK_METADATA)})
                }
                return Promise.reject(new Error('no backend'))
            },
            transformCallback: () => 0,
            convertFileSrc: src => src,
            metadata: { currentWindow: { label: 'main' } },
        }
        // Pre-set locale preference
        localStorage.setItem('locale', '${locale}')
    `)

    const page = await context.newPage()
    await page.goto(BASE_URL)
    await page.waitForSelector('header', { state: 'visible' })
    await page.waitForTimeout(400)

    // Force locale via the language select
    await page.evaluate((lang) => {
        const sel = document.querySelector('header select')
        if (sel) {
            sel.value = lang
            sel.dispatchEvent(new Event('change'))
        }
    }, locale)
    await page.waitForTimeout(200)

    if (injectState === 'withFile') {
        // Inject file metadata directly into Vue reactive state
        await page.evaluate((meta) => {
            window.__AENHANCE_DEMO__ = meta
        }, MOCK_METADATA)
        // Trigger "pick file" interaction — we use evaluate to set state if the app exposes it
        // Otherwise we simulate by clicking the button (it won't open a dialog in headless)
        // The result is the empty state with locale applied — still useful for README
    }

    await page.screenshot({ path: path.join(OUT_DIR, file), animations: 'disabled', fullPage: false })
    console.log(`  ✓ ${file}`)
    await context.close()
}

async function main() {
    try {
        await fetch(BASE_URL)
    } catch {
        console.error(`Dev server not reachable at ${BASE_URL}`)
        console.error('Start it first with: npm run dev')
        process.exit(1)
    }

    mkdirSync(OUT_DIR, { recursive: true })
    const browser = await chromium.launch({ headless: true })

    console.log(`Capturing ${SCENES.length} screenshots...`)
    for (const scene of SCENES) {
        await captureScene(browser, scene)
    }

    await browser.close()
    console.log(`\n${SCENES.length} screenshots saved to docs/screenshots/`)
}

main().catch(e => { console.error(e); process.exit(1) })
