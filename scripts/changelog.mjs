#!/usr/bin/env node
// Keeps CHANGELOG.md and the release notes in step.
//
//   node scripts/changelog.mjs release <version>   turn "Unreleased" into the version's section
//   node scripts/changelog.mjs notes <version>     print the release notes for a version
//   node scripts/changelog.mjs --check             verify the file's structure
//
// Changes go under "## Unreleased" as they are made. `release` (run by the
// Release workflow) refuses an empty "Unreleased" and, for a regular version,
// renames it to "## <version> – <date>" with a new empty "Unreleased" above.
// A pre-release (0.2.0-beta.1) leaves the file alone: its notes are the
// "Unreleased" section, which keeps growing until the regular release.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const file = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "CHANGELOG.md",
);
const UNRELEASED = "Unreleased";
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

function fail(message) {
  console.error(message);
  process.exit(1);
}

// Splits the file into the text before the first "## " heading and the
// sections, each with its name (first word of the heading) and body.
function parse(text) {
  const parts = text.split(/^## /m);
  const sections = parts.slice(1).map((part) => {
    const newline = part.indexOf("\n");
    const heading = newline < 0 ? part : part.slice(0, newline);
    return {
      heading,
      name: heading.split(/\s/)[0],
      body: newline < 0 ? "" : part.slice(newline + 1),
    };
  });
  return { head: parts[0], sections };
}

// The body without HTML comments and surrounding blank lines.
const content = (body) => body.replace(/<!--[\s\S]*?-->/g, "").trim();

function check({ sections }) {
  if (sections[0]?.name !== UNRELEASED)
    fail(`CHANGELOG.md must start with "## ${UNRELEASED}".`);
  const seen = new Set();
  for (const { heading, name } of sections.slice(1)) {
    if (!SEMVER.test(name))
      fail(`CHANGELOG.md: "## ${heading}" does not start with a version.`);
    if (seen.has(name)) fail(`CHANGELOG.md: version ${name} appears twice.`);
    seen.add(name);
  }
}

const [arg, rawVersion] = process.argv.slice(2);
const version = rawVersion?.replace(/^v/, "");
const text = readFileSync(file, "utf8");
const parsed = parse(text);
check(parsed);
const unreleased = parsed.sections[0];
const prerelease = version?.includes("-");

if (arg === "--check") {
  console.log("CHANGELOG.md is well-formed.");
} else if (
  (arg === "release" || arg === "notes") &&
  version &&
  SEMVER.test(version)
) {
  const existing = parsed.sections.find((section) => section.name === version);
  if (arg === "release") {
    if (existing) fail(`CHANGELOG.md already has a section for ${version}.`);
    if (!content(unreleased.body))
      fail(
        `"## ${UNRELEASED}" in CHANGELOG.md is empty: describe the changes first.`,
      );
    if (!prerelease) {
      const date = new Date().toISOString().slice(0, 10);
      const start = text.indexOf(`## ${unreleased.heading}`);
      const end = start + `## ${unreleased.heading}\n`.length;
      writeFileSync(
        file,
        `${text.slice(0, end)}\n## ${version} – ${date}\n${text.slice(end)}`,
      );
    }
  } else {
    const notes = content(
      (prerelease && !existing ? unreleased : existing)?.body ?? "",
    );
    if (!notes) fail(`CHANGELOG.md has no notes for ${version}.`);
    console.log(notes);
  }
} else {
  console.error(
    "Usage: node scripts/changelog.mjs <release <version> | notes <version> | --check>",
  );
  process.exit(2);
}
