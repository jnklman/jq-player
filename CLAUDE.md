# jQ Player – Notes for working on the code

## Rules

- **Reliability before features.** GO must always react immediately; a crash during a
  performance is the worst failure.
- **Audio thread (`Mixer::process` and everything below it):** no allocation, no locks,
  no I/O, no `println!`, no panics from indexing errors. Communication only through the
  `rtrb` queues; buffers are handed back via `Event::Finished` and freed in the control thread.
- The engine knows no cues, only voices. Cue logic belongs in a separate layer above it.
- Engine behaviour (loops, fades, devamp) is tested sample-accurately in `crates/jq-engine/src/tests.rs`;
  new engine features need a test there.
- **Localisation (German/English):** No text is hard-coded. Backend: `t!("key", name = value)`
  from `crates/jq-i18n`, texts in `crates/jq-i18n/locales/{en,de}.txt` (`key = text`, plurals via `key.one`/`key.other`
  and `count`); the language is global (`set_language`), never call it on the audio thread. UI: `t('key', {…})` from
  `app/src/lib/i18n` (reactive), texts in `en.ts` (authoritative, defines the keys) and `de.ts` (must have all of them);
  constants outside components hold keys or `labels()`/`options()`/getters, never finished texts.
  English is the fallback. The setting `LocalSettings.language` (`auto`/`en`/`de`) lives in the config file, not
  in localStorage. New language: add a catalog on each side, extend `Lang`/`LANGUAGES`. Code, comments and documentation
  are in English (German texts live only in the catalogs and the installer).
- Division of labour: core/backend/engine (Rust) is mostly Claude's, the UI (`app/src`)
  mostly the user's. The contract is `app/src/lib/api.ts`; every change to commands,
  events or serde types must be reflected there.
- UI styling exclusively with Tailwind CSS v4 (utility classes in the markup, no
  `<style>` blocks). Colours/fonts as theme tokens in `app/src/styles.css` (`@theme`),
  class sets used more than once in `app/src/lib/ui.ts`.
- UI building blocks are shadcn-svelte components in `app/src/lib/components/ui` (add new ones with
  `cd app && npx shadcn-svelte@latest add <name>`). The shadcn tokens (`primary`, `muted-foreground`,
  `popover`, …) are mapped onto the app palette in `styles.css`; `accent`/`muted` are hover
  surfaces there, the app blue is called `primary`. Icons from Phosphor (`~icons/ph/…`), not lucide.
- No `title` attribute: tooltips via `Tooltip` with `tip` (`app/src/lib/components/ui/tooltip/tooltip.svelte`; in button groups `ButtonGroup.Button` with `tip`), selection lists via
  `SimpleSelect`.
- Panels of the main view (movable, resizable) are registered in
  `app/src/components/layout/panels.ts` and build on `PanelFrame`; the backend stores the layout
  in the machine settings (`LocalSettings.layout`, only via `setLayout`).
- The UI holds no show state: it renders `show`/`state` events and sends
  commands. GO is fire-and-forget (`action`).
- App logic belongs in `app/src/lib`, not in components: operating commands (copy, paste,
  delete, add, undo, …) in `lib/commands.ts`, keyboard in `lib/shortcuts.ts`, so that a button
  and a shortcut do the same. Selection lists used more than once in `lib/options.ts`, number fields via
  `components/fields/NumberField.svelte`, dialogs with a draft via `components/EditDialog.svelte`.
- **Licence:** GPL-3.0-or-later (`LICENSE`). After changes to dependencies (`Cargo.lock`,
  `package-lock.json`) run `cd app && npm run licenses`: it generates `app/src/lib/licenses.json`
  (About dialog) and `THIRD_PARTY_LICENSES.md`; CI checks with `-- --check`. New dependencies
  must be GPLv3-compatible (no GPL-2.0-only, no proprietary or "non-commercial" licences).

- **Version:** The source is `[workspace.package] version` in `Cargo.toml` (all crates inherit it).
  Never change it by hand, use `node scripts/version.mjs <x.y.z|patch|minor|major>`: it updates
  `package.json`, `package-lock.json`, `tauri.conf.json` and `Cargo.lock` (`--check` verifies, CI
  does that). Release: start the "Release" workflow in GitHub Actions (it sets the version, commits, tags
  and triggers the build, which creates the GitHub release with NSIS, MSI, deb and AppImage files;
  the installers come in English and German).

## Commands

- `cargo test`, `cargo clippy --all-targets`, `cargo fmt`
- UI: `cd app && npm run lint` (ESLint; `lint:fix` fixes), `npm run format` (Prettier, check only;
  `format:fix` writes and also sorts Tailwind classes), `npm run check` (types), `npm test` (Vitest),
  `npm run app` (app in development mode). Mark class strings outside
  `class=` with the `tw` tag from `app/src/lib/ui.ts`.
- Linux needs `libasound2-dev` (cpal/midir: `jq-audio`, `jq-midi`, app) and the
  WebKitGTK packages for Tauri; `jq-engine`, `jq-media`, `jq-show`, `jq-core` build without them but need
  clang and a C++ compiler (Signalsmith Stretch in `jq-stretch`, via bindgen).
- On Linux the audio hosts PipeWire, JACK and PulseAudio are additionally built in; for that
  `libjack-jackd2-dev`, `libpipewire-0.3-dev` and clang. ASIO (Windows) is the feature `asio`
  (ASIO SDK in `CPAL_ASIO_DIR`, LLVM). cpal picks the default host (Linux: PipeWire, PulseAudio,
  otherwise ALSA). PulseAudio is not offered next to PipeWire (cpal sets a fixed
  surround channel layout, `aux` sinks then mix everything down to 1/2).
