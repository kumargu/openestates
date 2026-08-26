import { readFileSync, readdirSync, statSync } from 'node:fs'
import { dirname, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { defineConfig, loadEnv, type Plugin } from 'vite'
import react from '@vitejs/plugin-react'
import { normalizePublicOrigin } from './src/lib/publicUrls.ts'

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
const publicRoot = resolve(frontendRoot, 'public')
const DEPLOYABLE_PUBLIC_ROOTS = ['favicon.svg', 'landing', 'story-lab']

function filesUnder(path: string): string[] {
  if (!statSync(path).isDirectory()) return [path]
  return readdirSync(path).flatMap((entry) => filesUnder(resolve(path, entry)))
}

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

function productionOrigins(mode: string): { apiOrigin: string; siteOrigin: string } {
  const fileEnv = loadEnv(mode, frontendRoot, '')
  const readValue = (name: string) => process.env[name] ?? fileEnv[name]
  const required = mode === 'production'
  return {
    apiOrigin: normalizePublicOrigin(readValue('VITE_API_BASE'), 'VITE_API_BASE', {
      required,
      httpsOnly: required,
    }),
    siteOrigin: normalizePublicOrigin(readValue('VITE_SITE_URL'), 'VITE_SITE_URL', {
      required,
      httpsOnly: required,
    }),
  }
}

function robotsPlugin(apiOrigin: string): Plugin {
  return {
    name: 'openestates-robots',
    generateBundle() {
      this.emitFile({
        type: 'asset',
        fileName: 'robots.txt',
        source: [
          'User-agent: *',
          'Allow: /',
          `Sitemap: ${apiOrigin}/api/sitemap.xml`,
          '',
        ].join('\n'),
      })
    },
  }
}

function deployablePublicAssetsPlugin(): Plugin {
  return {
    name: 'openestates-deployable-public-assets',
    generateBundle() {
      for (const root of DEPLOYABLE_PUBLIC_ROOTS) {
        for (const path of filesUnder(resolve(publicRoot, root))) {
          this.emitFile({
            type: 'asset',
            fileName: relative(publicRoot, path).split(sep).join('/'),
            source: readFileSync(path),
          })
        }
      }
    },
  }
}

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
  const origins = productionOrigins(mode)
  if (command === 'build') validatePromotedMedia()

  return {
    publicDir: command === 'build' ? false : 'public',
    plugins: [
      react(),
      deployablePublicAssetsPlugin(),
      robotsPlugin(origins.apiOrigin),
    ],
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
