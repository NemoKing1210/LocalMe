/**
 * The icon a file is shown with, chosen from its extension.
 *
 * These come from the `vscode-icons` set through `unplugin-icons`, which is the one icon library
 * this project uses and the one place it does not draw its own glyphs: `src/ui/icons.ts` holds the
 * application's own ~40 interface marks, while a file-type badge is a picture of *someone else's
 * format* — there are a hundred of them, they change, and a hand-drawn approximation of a PDF is
 * worse than the one every file manager on the machine already shows. The plugin resolves the
 * import at build time, so only the icons listed here end up in the bundle.
 *
 * Colour is deliberate. A file list is where a reader scans for shape and colour rather than for
 * text, and these badges are drawn for exactly that; the interface's own marks stay on the tonal
 * palette.
 */
import type { Component } from 'vue';

import ArchiveIcon from '~icons/vscode-icons/file-type-zip';
import AudioIcon from '~icons/vscode-icons/file-type-audio';
import BinaryIcon from '~icons/vscode-icons/file-type-binary';
import CIcon from '~icons/vscode-icons/file-type-c';
import CppIcon from '~icons/vscode-icons/file-type-cpp';
import CsharpIcon from '~icons/vscode-icons/file-type-csharp';
import CssIcon from '~icons/vscode-icons/file-type-css';
import CsvIcon from '~icons/vscode-icons/file-type-csv';
import DartIcon from '~icons/vscode-icons/file-type-dartlang';
import DatabaseIcon from '~icons/vscode-icons/file-type-db';
import ElixirIcon from '~icons/vscode-icons/file-type-elixir';
import ExcelIcon from '~icons/vscode-icons/file-type-excel';
import FontIcon from '~icons/vscode-icons/file-type-font';
import GoIcon from '~icons/vscode-icons/file-type-go';
import HtmlIcon from '~icons/vscode-icons/file-type-html';
import ImageIcon from '~icons/vscode-icons/file-type-image';
import InitIcon from '~icons/vscode-icons/file-type-ini';
import JarIcon from '~icons/vscode-icons/file-type-jar';
import JavaIcon from '~icons/vscode-icons/file-type-java';
import JavaScriptIcon from '~icons/vscode-icons/file-type-js';
import JsonIcon from '~icons/vscode-icons/file-type-json';
import KotlinIcon from '~icons/vscode-icons/file-type-kotlin';
import LogIcon from '~icons/vscode-icons/file-type-log';
import MarkdownIcon from '~icons/vscode-icons/file-type-markdown';
import PdfIcon from '~icons/vscode-icons/file-type-pdf2';
import PhpIcon from '~icons/vscode-icons/file-type-php';
import PowerShellIcon from '~icons/vscode-icons/file-type-powershell';
import PowerPointIcon from '~icons/vscode-icons/file-type-powerpoint';
import PythonIcon from '~icons/vscode-icons/file-type-python';
import ReactIcon from '~icons/vscode-icons/file-type-reactjs';
import RustIcon from '~icons/vscode-icons/file-type-rust';
import SassIcon from '~icons/vscode-icons/file-type-scss';
import ShellIcon from '~icons/vscode-icons/file-type-shell';
import SourceIcon from '~icons/vscode-icons/file-type-source';
import SqlIcon from '~icons/vscode-icons/file-type-sqlite';
import SvgIcon from '~icons/vscode-icons/file-type-svg';
import SwiftIcon from '~icons/vscode-icons/file-type-swift';
import TextIcon from '~icons/vscode-icons/file-type-text';
import TexIcon from '~icons/vscode-icons/file-type-tex';
import TomlIcon from '~icons/vscode-icons/file-type-toml';
import TypeScriptIcon from '~icons/vscode-icons/file-type-typescript';
import UnknownIcon from '~icons/vscode-icons/default-file';
import VideoIcon from '~icons/vscode-icons/file-type-video';
import VueIcon from '~icons/vscode-icons/file-type-vue';
import WordIcon from '~icons/vscode-icons/file-type-word';
import XmlIcon from '~icons/vscode-icons/file-type-xml';
import YamlIcon from '~icons/vscode-icons/file-type-yaml';

/**
 * Extension → icon.
 *
 * Several extensions share one icon wherever the set has no distinct badge for them: `.rar` and
 * `.7z` are shown as archives rather than pretending to be ZIP files, and an installer is shown as
 * a binary rather than as a Debian package.
 */
const BY_EXTENSION: Readonly<Record<string, Component>> = {
  // Documents
  pdf: PdfIcon,
  doc: WordIcon,
  docx: WordIcon,
  odt: WordIcon,
  rtf: WordIcon,
  pages: WordIcon,
  xls: ExcelIcon,
  xlsx: ExcelIcon,
  ods: ExcelIcon,
  numbers: ExcelIcon,
  ppt: PowerPointIcon,
  pptx: PowerPointIcon,
  odp: PowerPointIcon,
  key: PowerPointIcon,
  txt: TextIcon,
  log: LogIcon,
  md: MarkdownIcon,
  markdown: MarkdownIcon,
  csv: CsvIcon,
  tsv: CsvIcon,
  tex: TexIcon,

  // Data and configuration
  json: JsonIcon,
  jsonc: JsonIcon,
  json5: JsonIcon,
  xml: XmlIcon,
  xsl: XmlIcon,
  plist: XmlIcon,
  yaml: YamlIcon,
  yml: YamlIcon,
  toml: TomlIcon,
  ini: InitIcon,
  cfg: InitIcon,
  conf: InitIcon,
  env: InitIcon,
  sql: SqlIcon,
  db: DatabaseIcon,
  sqlite: DatabaseIcon,
  sqlite3: DatabaseIcon,
  mdb: DatabaseIcon,
  lock: TextIcon,

  // Images, sound, video, fonts
  png: ImageIcon,
  jpg: ImageIcon,
  jpeg: ImageIcon,
  gif: ImageIcon,
  webp: ImageIcon,
  bmp: ImageIcon,
  avif: ImageIcon,
  heic: ImageIcon,
  ico: ImageIcon,
  tif: ImageIcon,
  tiff: ImageIcon,
  psd: ImageIcon,
  svg: SvgIcon,
  mp3: AudioIcon,
  wav: AudioIcon,
  flac: AudioIcon,
  ogg: AudioIcon,
  oga: AudioIcon,
  m4a: AudioIcon,
  aac: AudioIcon,
  opus: AudioIcon,
  wma: AudioIcon,
  mid: AudioIcon,
  midi: AudioIcon,
  aiff: AudioIcon,
  mp4: VideoIcon,
  m4v: VideoIcon,
  mkv: VideoIcon,
  avi: VideoIcon,
  mov: VideoIcon,
  webm: VideoIcon,
  wmv: VideoIcon,
  flv: VideoIcon,
  mpg: VideoIcon,
  mpeg: VideoIcon,
  '3gp': VideoIcon,
  ttf: FontIcon,
  otf: FontIcon,
  woff: FontIcon,
  woff2: FontIcon,
  eot: FontIcon,

  // Archives and packages
  zip: ArchiveIcon,
  rar: ArchiveIcon,
  '7z': ArchiveIcon,
  tar: ArchiveIcon,
  gz: ArchiveIcon,
  tgz: ArchiveIcon,
  bz2: ArchiveIcon,
  xz: ArchiveIcon,
  zst: ArchiveIcon,
  iso: ArchiveIcon,
  dmg: ArchiveIcon,
  exe: BinaryIcon,
  msi: BinaryIcon,
  msix: BinaryIcon,
  dll: BinaryIcon,
  so: BinaryIcon,
  dylib: BinaryIcon,
  bin: BinaryIcon,
  deb: BinaryIcon,
  rpm: BinaryIcon,
  apk: BinaryIcon,
  appimage: BinaryIcon,
  jar: JarIcon,

  // Source
  js: JavaScriptIcon,
  mjs: JavaScriptIcon,
  cjs: JavaScriptIcon,
  jsx: ReactIcon,
  ts: TypeScriptIcon,
  mts: TypeScriptIcon,
  cts: TypeScriptIcon,
  tsx: ReactIcon,
  vue: VueIcon,
  html: HtmlIcon,
  htm: HtmlIcon,
  css: CssIcon,
  scss: SassIcon,
  sass: SassIcon,
  less: CssIcon,
  py: PythonIcon,
  pyw: PythonIcon,
  rs: RustIcon,
  go: GoIcon,
  java: JavaIcon,
  kt: KotlinIcon,
  kts: KotlinIcon,
  swift: SwiftIcon,
  dart: DartIcon,
  c: CIcon,
  h: CIcon,
  cpp: CppIcon,
  cc: CppIcon,
  cxx: CppIcon,
  hpp: CppIcon,
  cs: CsharpIcon,
  rb: SourceIcon,
  php: PhpIcon,
  pl: SourceIcon,
  pm: SourceIcon,
  lua: SourceIcon,
  ex: ElixirIcon,
  exs: ElixirIcon,
  sh: ShellIcon,
  bash: ShellIcon,
  zsh: ShellIcon,
  fish: ShellIcon,
  ps1: PowerShellIcon,
  bat: ShellIcon,
  cmd: ShellIcon,
};

/**
 * The icon for a file name.
 *
 * A name with no extension, or with one this build does not know, gets the generic file mark
 * rather than a guess: a wrong badge is worse than a plain one, because the reader stops looking
 * at the name.
 */
export function fileIconFor(name: string): Component {
  const dot = name.lastIndexOf('.');
  if (dot <= 0 || dot === name.length - 1) return UnknownIcon;
  const extension = name.slice(dot + 1).toLowerCase();
  return BY_EXTENSION[extension] ?? UnknownIcon;
}
