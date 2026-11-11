// Collects the licenses of everything that ends up in the app (Rust crates and
// npm packages) into `src/lib/licenses.json`, which the "Über" dialog shows,
// and `THIRD_PARTY_LICENSES.md` in the repository root.
//
// Run `npm run licenses` after changing dependencies (needs `cargo` and the
// installed crate sources and `node_modules`). `npm run licenses -- --check`
// only verifies that the committed files are current.
//
// Rust: the normal (not dev or build) dependencies of the app for the two
// release targets, with the `asio` feature. npm: the non-dev packages of the lockfile.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const rootDir = resolve(appDir, '..');
const jsonFile = join(appDir, 'src/lib/licenses.json');
const mdFile = join(rootDir, 'THIRD_PARTY_LICENSES.md');
const check = process.argv.includes('--check');

const TARGETS = ['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu'];
const LICENSE_FILE = /^(licen[cs]e|copying|unlicense|notice)([-_.].*)?$/i;

/** Contents of the license files directly in `dir`, without duplicates. */
function licenseTexts(dir) {
  const texts = [];
  for (const name of readdirSync(dir).sort()) {
    if (!LICENSE_FILE.test(name)) continue;
    const path = join(dir, name);
    let text;
    try {
      text = readFileSync(path, 'utf8');
    } catch {
      continue; // A directory.
    }
    text = text.replace(/\r\n/g, '\n').trim();
    if (text && !texts.includes(text)) texts.push(text);
  }
  return texts;
}

// --- Rust ------------------------------------------------------------------

function cargoPackages() {
  const found = new Map();
  for (const target of TARGETS) {
    const meta = JSON.parse(
      execFileSync(
        'cargo',
        ['metadata', '--format-version', '1', '--locked', '--all-features', '--filter-platform', target],
        { cwd: rootDir, maxBuffer: 1 << 29, encoding: 'utf8' },
      ),
    );
    const byId = new Map(meta.packages.map((p) => [p.id, p]));
    const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
    const root = meta.packages.find((p) => p.name === 'jq-player').id;
    const workspace = new Set(meta.workspace_members);
    const seen = new Set([root]);
    const todo = [root];
    while (todo.length) {
      for (const dep of nodes.get(todo.pop()).deps) {
        // Normal dependencies only: `kind` is null for those, "dev" and "build" otherwise.
        if (!dep.dep_kinds.some((k) => k.kind === null) || seen.has(dep.pkg)) continue;
        seen.add(dep.pkg);
        todo.push(dep.pkg);
      }
    }
    for (const id of seen) {
      if (workspace.has(id)) continue;
      const p = byId.get(id);
      found.set(`${p.name}@${p.version}`, {
        name: p.name,
        version: p.version,
        license: p.license ?? null,
        authors: p.authors ?? [],
        repository: p.repository ?? p.homepage ?? null,
        dir: dirname(p.manifest_path),
      });
    }
  }
  return [...found.values()];
}

// --- npm -------------------------------------------------------------------

function npmPackages() {
  const lock = JSON.parse(readFileSync(join(appDir, 'package-lock.json'), 'utf8'));
  const found = new Map();
  for (const [path, entry] of Object.entries(lock.packages)) {
    if (!path || entry.dev) continue;
    const dir = join(appDir, path);
    const manifest = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
    const repository = typeof manifest.repository === 'string' ? manifest.repository : manifest.repository?.url;
    found.set(`${manifest.name}@${manifest.version}`, {
      name: manifest.name,
      version: manifest.version,
      license: typeof manifest.license === 'string' ? manifest.license : (manifest.license?.type ?? null),
      authors: [typeof manifest.author === 'string' ? manifest.author : manifest.author?.name].filter(Boolean),
      repository: repository?.replace(/^git\+/, '').replace(/\.git$/, '') ?? null,
      dir,
    });
  }
  return [...found.values()];
}

// --- Assemble ----------------------------------------------------------------

const GPL = readFileSync(join(rootDir, 'LICENSE'), 'utf8').replace(/\r\n/g, '\n').trim();

/** Things that are not in a package manager. */
const extra = [
  {
    name: 'Steinberg ASIO SDK',
    version: '2.3.4',
    license: 'GPL-3.0-only',
    repository: 'https://www.steinberg.net/developers/',
    texts: [GPL],
    note: 'Nur in der Windows-Version mit ASIO-Ausgabe. ASIO ist eine Marke und Software der Steinberg Media Technologies GmbH.',
  },
];

const sections = [
  { id: 'rust', title: 'Rust-Bibliotheken', packages: cargoPackages() },
  { id: 'npm', title: 'Oberfläche (npm)', packages: npmPackages() },
  { id: 'extra', title: 'Weitere', packages: extra },
];

// --- Packages without a license file -------------------------------------------
// Their license sits in the repository root, not in the published package. The
// standard text then stands in, with the copyright holders from the manifest.

const MIT = (who) => `MIT License

Copyright (c) ${who}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;

const BSD3 = (who) => `BSD 3-Clause License

Copyright (c) ${who}

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.`;

/** A text of `marker` found in any package that has one (Apache-2.0 and MPL-2.0 carry no copyright line). */
function sharedText(all, marker) {
  for (const p of all) for (const t of p.found ?? []) if (t.includes(marker) && t.length > 8000) return t;
  return null;
}

/** Standard text for the license expression of a package without a license file; null if there is none. */
function standardText(p, all) {
  const who = p.authors.map((a) => a.replace(/\s*<[^>]*>/, '')).join(', ') || `the ${p.name} authors`;
  const ids = (p.license ?? '').split(/\s+(?:OR|AND)\s+|\//).map((id) => id.replace(/[()]/g, '').trim());
  for (const id of ids) {
    if (id === 'MIT') return MIT(who);
    if (id === 'BSD-3-Clause') return BSD3(who);
    if (id === 'Apache-2.0') return sharedText(all, 'Apache License');
    if (id === 'MPL-2.0') return sharedText(all, 'Mozilla Public License');
  }
  return null;
}

const missing = [];
for (const section of sections) for (const p of section.packages) p.found = p.texts ?? licenseTexts(p.dir);
const everyPackage = sections.flatMap((section) => section.packages);
const texts = []; // Each distinct text once; packages refer to it by index.
const index = new Map();
const out = sections.map((section) => ({
  id: section.id,
  title: section.title,
  packages: section.packages
    .map((p) => {
      const own = [...p.found];
      let standard = false;
      if (!own.length) {
        const text = standardText(p, everyPackage);
        if (text) {
          own.push(text);
          standard = true;
        } else missing.push(`${section.id}: ${p.name}@${p.version} (${p.license})`);
      }
      return {
        name: p.name,
        version: p.version,
        license: p.license,
        ...(p.repository && { repository: p.repository }),
        ...((p.note || standard) && {
          note:
            p.note ??
            'Das Paket enthält keine Lizenzdatei; es gilt der Standardtext der Lizenz mit den Urhebern laut Paketangabe.',
        }),
        texts: own.map((text) => {
          const key = createHash('sha256').update(text).digest('hex');
          if (!index.has(key)) index.set(key, texts.push(text) - 1);
          return index.get(key);
        }),
      };
    })
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version)),
}));

const gpl = index.get(createHash('sha256').update(GPL).digest('hex'));
const json = JSON.stringify({ gpl, sections: out, texts }) + '\n';

let md = '# Lizenzen von Drittanbieter-Komponenten\n\n';
md += 'jQ Player steht unter der GNU General Public License Version 3 oder später (siehe `LICENSE`).\n';
md += 'Diese Datei wird mit `npm run licenses` (in `app/`) erzeugt; sie führt die Bibliotheken auf, die in\n';
md += 'der Anwendung stecken, mit ihren Lizenztexten.\n\n';
for (const section of out) {
  md += `## ${section.title}\n\n`;
  for (const p of section.packages) {
    md += `### ${p.name} ${p.version}\n\n`;
    md += `Lizenz: ${p.license ?? 'unbekannt'}${p.repository ? ` · ${p.repository}` : ''}\n\n`;
    if (p.note) md += `${p.note}\n\n`;
    if (p.name === 'Steinberg ASIO SDK') md += 'Lizenztext: GPL-3.0, siehe `LICENSE`.\n\n';
    else for (const i of p.texts) md += '```text\n' + texts[i] + '\n```\n\n';
  }
}

if (check) {
  const current = (file) => (existsSync(file) ? readFileSync(file, 'utf8') : '');
  if (current(jsonFile) !== json || current(mdFile) !== md) {
    console.error('Lizenzdateien sind veraltet: `npm run licenses` ausführen.');
    process.exit(1);
  }
} else {
  writeFileSync(jsonFile, json);
  writeFileSync(mdFile, md);
}
const count = out.reduce((n, s) => n + s.packages.length, 0);
console.log(`${count} Pakete, ${texts.length} verschiedene Lizenztexte, ${json.length >> 10} KiB`);
if (missing.length) console.log(`Ohne Lizenzdatei (${missing.length}):\n  ${missing.join('\n  ')}`);
