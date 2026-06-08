/** @type {import('tailwindcss').Config} */
export default {
    content: ['./index.html', './src/**/*.{vue,js,ts,jsx,tsx}'],
    theme: {
        extend: {
            fontFamily: {
                display: ['Avenir Next', 'Avenir', 'Segoe UI', 'sans-serif'],
                body: ['Avenir Next', 'Avenir', 'Segoe UI', 'sans-serif'],
            },
            boxShadow: {
                glow: '0 24px 90px rgba(24, 65, 54, 0.18)',
            },
        },
    },
    plugins: [],
}
