const fs = require('node:fs');
const path = require('node:path');
const read = name => {
  const file = path.join(__dirname, name + '.jsonl');
  return fs.existsSync(file) ? fs.readFileSync(file, 'utf8').trim().split('\n').filter(Boolean).map(JSON.parse).filter(x => x.event === 'result') : [];
};
const data = {guard: 'pass', baseline: read('baseline-durable'), candidate: read('candidate-3'),
  prefix_candidate: read('candidate-2'),
  discarded_candidate: read('candidate-1'),
  holdout: [...read('holdout-3-scourge'), ...read('holdout-3-ritualist'),
    ...read('holdout-3-or-scourge').map(x => ({...x, route: 'openrouter'})),
    ...read('holdout-3-or-ritualist').map(x => ({...x, route: 'openrouter'}))],
  previous_holdout: [...read('holdout-reaper'), ...read('holdout-harbinger')],
  native_gemini: [...read('native-gemini'), ...read('native-gemini-diagnostic'), ...read('native-gemini-alternate')]};
fs.writeFileSync(path.join(__dirname, 'measurements.json'), JSON.stringify(data, null, 2) + '\n');
for (const phase of ['baseline', 'candidate', 'holdout']) {
  for (const model of [...new Set(data[phase].map(x => x.model))]) {
    const rows = data[phase].filter(x => x.model === model);
    const passes = rows.filter(x => x.status === 'PASS');
    console.log(JSON.stringify({phase, model, passed: passes.length, attempts: rows.length,
      seconds: rows.map(x => x.seconds), status: rows.map(x => x.status), wire: rows.map(x => x.wire)}));
  }
}
