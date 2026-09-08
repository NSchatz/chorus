// Did every claim run, and was every one of them shown going red?
//
// The run itself cannot answer either question. A deleted assertion leaves a
// green run with one fewer test in it, and an assertion that cannot fail leaves
// a green run that proves nothing. So every assertion records the claim it
// proved and every demonstration records the claim it broke, and this reads the
// two lists back against the declared set.
//
// Each line also carries the file it was written from, and this refuses one
// written from the wrong file: `proves` and `demonstrates` are exported to both
// spec files, so without that a `demonstrates()` call sitting in ui.spec.js
// beside the real page would satisfy the count while demonstrating nothing.
//
//   node check-claims.js <ledger>

const { CLAIMS, RECORDED_IN, readLedger } = require("./claims");

const ledgerPath = process.argv[2];
const { claims, demos, misfiled } = readLedger(ledgerPath);
const declared = Object.keys(CLAIMS).sort();

const missing = declared.filter((id) => !claims.includes(id));
const undeclared = claims.filter((id) => !declared.includes(id));
const undemonstrated = claims.filter((id) => !demos.includes(id));

console.log(`declared claims:      ${declared.length}`);
console.log(`claims that ran:      ${claims.length}`);
console.log(`demonstrations that ran: ${demos.length}`);

let failures = 0;

if (missing.length) {
  console.log("FAIL these declared claims did not run in this invocation:");
  missing.forEach((id) => console.log(`    ${id}: ${CLAIMS[id]}`));
  failures += 1;
}

if (undeclared.length) {
  console.log("FAIL these claims were recorded but are not declared:");
  undeclared.forEach((id) => console.log(`    ${id}`));
  failures += 1;
}

if (undemonstrated.length) {
  console.log(
    "FAIL these claims ran with no demonstration beside them, so nothing shows the measurement can fail:"
  );
  undemonstrated.forEach((id) => console.log(`    ${id}: ${CLAIMS[id]}`));
  failures += 1;
}

// A demonstration is only evidence if it is pointed at a page built to break
// the claim, and an assertion is only evidence if it is pointed at the page a
// real server served. The two live in different files on purpose, and a line
// written from the wrong one is discounted and named rather than counted.
if (misfiled.length) {
  console.log(
    "FAIL these ledger lines were recorded from a file that does not record that kind:"
  );
  misfiled.forEach((line) =>
    console.log(
      `    ${line.kind} ${line.id} was recorded in ${line.from}, and a ${line.kind} belongs in ${RECORDED_IN[line.kind]}`
    )
  );
  failures += 1;
}

// The criterion's own arithmetic: fewer demonstrations than rendered claims is a
// failure whatever the per-claim mapping says.
if (demos.length < claims.length) {
  console.log(
    `FAIL ${demos.length} demonstrations ran against ${claims.length} rendered claims`
  );
  failures += 1;
}

if (failures === 0) {
  console.log(
    `pass all ${claims.length} rendered claims ran, and each was shown going red on a page that breaks it`
  );
  process.exit(0);
}
process.exit(1);
