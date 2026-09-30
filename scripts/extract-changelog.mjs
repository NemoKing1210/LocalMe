import { fail, parseArgs } from './lib/cli.mjs';
import {
  extractChangelogSection,
  formatReleaseNotes,
  latestChangelogVersion,
  packageVersion,
} from './lib/versions.mjs';

const USAGE = `Usage:
  npm run changelog [-- <version>] [--github]

Prints the release notes for a version — the latest CHANGELOG.md section by
default — for use as a tag message or a GitHub Release body.`;

const args = parseArgs(process.argv.slice(2));
if (args.has('help')) {
  console.log(USAGE);
  process.exit(0);
}

const version = args.positionals[0] ?? latestChangelogVersion() ?? packageVersion();
const section = extractChangelogSection(version);

if (!section) {
  fail(`CHANGELOG.md has no ## [${version}] section`);
}

process.stdout.write(formatReleaseNotes(section, { footer: args.has('github') }));
