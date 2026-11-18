// Stand-in editor for the M5 walkthrough: writes the arguments it was started
// with to the JSON file named by ALDENTE_M5_EDITOR_LOG, then exits. Lets the
// walkthrough prove "jump to the editor" without opening a real editor.
//
// Example: `node record-editor.mjs src/a.clj 12 1` with the log set to
// /tmp/x.json writes `["src/a.clj","12","1"]` to /tmp/x.json.

import { writeFileSync } from 'node:fs';

const log = process.env.ALDENTE_M5_EDITOR_LOG;
if (!log) {
  console.error('record-editor: ALDENTE_M5_EDITOR_LOG is not set');
  process.exit(2);
}
writeFileSync(log, JSON.stringify(process.argv.slice(2)));
