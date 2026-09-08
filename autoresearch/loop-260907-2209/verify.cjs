const fs = require('node:fs');
const path = require('node:path');
const file = path.join(__dirname, 'measurements.json');
const data = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : {};
const models = ['google/gemini-3.8-flash', 'inclusionai/ling-3.0-flash-sante:free'];
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
let complete = data.guard === 'pass';
for (const model of models) {
  const baseline = (data.baseline || []).filter(x => x.model === model);
  const candidate = (data.candidate || []).filter(x => x.model === model);
  const valid = rows => rows.length === 3 && rows.every(x => x.status === 'PASS' && Number.isFinite(x.seconds));
  const ready = valid(baseline) && valid(candidate);
  complete &&= ready && median(candidate.map(x => x.seconds)) < median(baseline.map(x => x.seconds));
  console.log(JSON.stringify({model, baseline: baseline.length, candidate: candidate.length,
    baseline_median: valid(baseline) ? median(baseline.map(x => x.seconds)) : null,
    candidate_median: valid(candidate) ? median(candidate.map(x => x.seconds)) : null}));
}
complete &&= (data.holdout || []).length >= 2 && data.holdout.every(x => x.status === 'PASS');
console.log(complete ? 'CONVERGED' : 'PENDING');
process.exitCode = complete ? 0 : 1;
