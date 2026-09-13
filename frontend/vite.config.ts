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

type StaticApiFixtureManifest = {
  responses: Array<{
    endpoint: string
    file: string
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

function waterfordApiFixturePlugin(mode: string): Plugin {
  return {
    name: 'waterford-api-fixture',
    configureServer(server) {
      if (mode !== 'waterford') return

      const fixtureRoot = resolve(frontendRoot, 'fixtures/prestige-waterford-api')
      const manifest = JSON.parse(
        readFileSync(resolve(fixtureRoot, 'manifest.json'), 'utf8'),
      ) as StaticApiFixtureManifest
      const responses = new Map(manifest.responses.map((response) => [
        response.endpoint,
        readFileSync(resolve(fixtureRoot, response.file)),
      ]))

      server.middlewares.use((request, response, next) => {
        if (request.method !== 'GET') return next()
        const pathname = new URL(
          request.url ?? '/',
          'http://waterford-fixture.local',
        ).pathname
        const body = responses.get(pathname)
        if (!body) return next()

        response.statusCode = 200
        response.setHeader('Content-Type', 'application/json; charset=utf-8')
        response.setHeader('X-OpenEstates-Fixture', 'prestige-waterford')
        response.end(body)
      })
    },
  }
}

// https://vite.dev/config/
export default defineConfig(({ command, mode }) => {
  const origins = productionOrigins(mode)
  if (command === 'build') validatePromotedMedia()

  return {
    // Vercel's environment is server-side by default. Expose only the
    // non-secret environment name so preview URLs can opt into review fixtures
    // without making fixtures reachable from the production deployment.
    define: {
      'import.meta.env.VITE_VERCEL_ENV': JSON.stringify(process.env.VERCEL_ENV ?? ''),
    },
    publicDir: command === 'build' ? false : 'public',
    plugins: [
      waterfordApiFixturePlugin(mode),
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
