import { readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

type FrontendMediaManifest = {
  version: number
  bundle_version: string
  assets: Array<{
    url: string
    content_sha256: string
    size_bytes: number
  }>
}

const frontendRoot = dirname(fileURLToPath(import.meta.url))

function validatePromotedMedia(): FrontendMediaManifest {
  const manifestPath = resolve(frontendRoot, 'media-manifest.json')
  const manifest = JSON.parse(readFileSync(manifestPath, 'utf8')) as FrontendMediaManifest

  // Lake-served `/media/*` objects do not appear in this inventory. Promotion
  // has already verified those bytes and hashes; this build gate only certifies
  // that property media has not slipped back into the frontend deployment.
  if (manifest.assets.length > 0) {
    throw new Error(
      `Bundle ${manifest.bundle_version} still contains ${manifest.assets.length} frontend-packaged media assets; use the lake-backed /media route`,
    )
  }

  return manifest
}

// https://vite.dev/config/
export default defineConfig(({ command }) => {
  if (command === 'build') validatePromotedMedia()

  return {
    plugins: [react()],
    server: {
      fs: {
        allow: [resolve(frontendRoot, '..')],
      },
      proxy: {
        '/api': 'http://127.0.0.1:4000',
        '/media': 'http://127.0.0.1:4000',
      },
    },
  }
})
