#!/usr/bin/env node
// ============================================================
// Sift - Version Sync
// package.json is the single source of truth for the app version.
//
//   node scripts/sync-version.mjs <version>   write <version> everywhere
//   node scripts/sync-version.mjs --check     fail if any file disagrees
//
// Cargo.lock is not touched here; `cargo check` refreshes it.
// ============================================================

import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?$/
// First `version = "..."` line, i.e. the one under [package]
const CARGO_VERSION = /^version = "([^"]*)"/m

const read = (file) => readFileSync(resolve(ROOT, file), 'utf8')
const write = (file, text) => writeFileSync(resolve(ROOT, file), text)
const readJson = (file) => JSON.parse(read(file))
const writeJson = (file, data) => write(file, JSON.stringify(data, null, 2) + '\n')

const TARGETS = [
  {
    file: 'package.json',
    get: () => readJson('package.json').version,
    set: (v) => writeJson('package.json', { ...readJson('package.json'), version: v }),
  },
  {
    file: 'package-lock.json',
    get: () => {
      const lock = readJson('package-lock.json')
      return lock.version === lock.packages[''].version ? lock.version : null
    },
    set: (v) => {
      const lock = readJson('package-lock.json')
      writeJson('package-lock.json', {
        ...lock,
        version: v,
        packages: { ...lock.packages, '': { ...lock.packages[''], version: v } },
      })
    },
  },
  {
    file: 'src-tauri/tauri.conf.json',
    get: () => readJson('src-tauri/tauri.conf.json').version,
    set: (v) =>
      writeJson('src-tauri/tauri.conf.json', {
        ...readJson('src-tauri/tauri.conf.json'),
        version: v,
      }),
  },
  {
    file: 'src-tauri/Cargo.toml',
    get: () => read('src-tauri/Cargo.toml').match(CARGO_VERSION)?.[1] ?? null,
    set: (v) =>
      write(
        'src-tauri/Cargo.toml',
        read('src-tauri/Cargo.toml').replace(CARGO_VERSION, `version = "${v}"`),
      ),
  },
]

function check() {
  const expected = TARGETS[0].get()
  const mismatches = TARGETS.filter((t) => t.get() !== expected)
  if (mismatches.length === 0) {
    console.log(`All versions match ${expected}`)
    return
  }
  for (const t of mismatches) {
    console.error(`${t.file}: ${t.get()} (expected ${expected})`)
  }
  console.error('Run: node scripts/sync-version.mjs ' + expected)
  process.exit(1)
}

function sync(rawVersion) {
  const version = rawVersion.replace(/^v/, '')
  if (!SEMVER.test(version)) {
    console.error(`Invalid semver: '${rawVersion}' (expected e.g. 1.2.0 or 1.0.0-beta.1)`)
    process.exit(1)
  }
  for (const t of TARGETS) {
    t.set(version)
    console.log(`${t.file} -> ${version}`)
  }
}

const arg = process.argv[2]
if (!arg) {
  console.error('Usage: node scripts/sync-version.mjs <version> | --check')
  process.exit(1)
}
if (arg === '--check') check()
else sync(arg)
