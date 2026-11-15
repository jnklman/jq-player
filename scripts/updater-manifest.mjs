// Writes the update manifest (`latest.json`) the app's updater reads from the
// latest GitHub release. Used by the Build workflow's release job.
//
//   node scripts/updater-manifest.mjs <artifact dir> <owner/repo> <tag> > latest.json
//
// Looks for the signed updater files in the artifact dir: the regular NSIS
// installer (`*-setup.exe`, not the offline one, which has no signature) for
// Windows, the English MSI for copies installed from an MSI (either language:
// they upgrade each other), the AppImage for Linux and the deb package for
// copies installed from it, each with its `.sig`. The updater looks for
// `{os}-{arch}-{bundle type}` first, then `{os}-{arch}`: a deb or MSI install
// must get its own package type. Pre-releases have no MSI (see build.yml). The URLs are those
// GitHub gives the release assets (spaces in file names become dots).

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { basename, join } from 'node:path';

const [dir, repo, tag] = process.argv.slice(2);
if (!dir || !repo || !tag) {
  console.error('usage: updater-manifest.mjs <artifact dir> <owner/repo> <tag>');
  process.exit(2);
}

function* walk(path) {
  for (const name of readdirSync(path)) {
    const full = join(path, name);
    if (statSync(full).isDirectory()) yield* walk(full);
    else yield full;
  }
}

const signed = [...walk(dir)].filter((file) => file.endsWith('.sig')).map((file) => file.slice(0, -4));
const pick = (pattern) => signed.filter((file) => pattern.test(basename(file)));

const targets = {
  'windows-x86_64': pick(/-setup\.exe$/),
  'windows-x86_64-msi': pick(/_en-US\.msi$/),
  'linux-x86_64': pick(/\.AppImage$/),
  'linux-x86_64-deb': pick(/\.deb$/),
};
const prerelease = tag.includes('-');
if (prerelease) delete targets['windows-x86_64-msi'];
const platforms = {};
for (const [target, files] of Object.entries(targets)) {
  if (files.length !== 1) {
    console.error(`expected exactly one signed updater file for ${target}, found ${files.length}`);
    process.exit(1);
  }
  const [file] = files;
  platforms[target] = {
    signature: readFileSync(`${file}.sig`, 'utf8').trim(),
    url: `https://github.com/${repo}/releases/download/${tag}/${basename(file).replaceAll(' ', '.')}`,
  };
}

console.log(
  JSON.stringify(
    {
      version: tag.replace(/^v/, ''),
      notes: `https://github.com/${repo}/releases/tag/${tag}`,
      pub_date: new Date().toISOString(),
      platforms,
    },
    null,
    2,
  ),
);
