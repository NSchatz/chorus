// The committed record of the frontend conventions, read and refused.
//
// `documentation/frontend-conventions.md` on the umbrella side has eleven
// clauses. `docs/frontend-conventions-record.md` here maps each of them to
// either the rendered assertion that proves it or a named exemption saying why
// that clause cannot apply to this surface. This reads that record and exits
// non-zero, naming the clause, when any of the eleven is
//
//   absent from the record,
//   carrying both an assertion and an exemption, or carrying neither,
//   naming an assertion that is not a declared claim, or
//   naming an assertion that did not run in this invocation.
//
// The last of those is the one that matters most: a record is only worth
// committing if it cannot go on claiming an assertion after the assertion has
// stopped running.
//
//   node check-clause-record.js <record.md> <ledger>

const fs = require("fs");
const { CLAIMS, readLedger } = require("./claims");

const CLAUSES = Array.from({ length: 11 }, (_, i) => `F${i + 1}`);

const recordPath = process.argv[2];
const ledgerPath = process.argv[3];

function fail(message) {
  console.log(`FAIL ${message}`);
  return 1;
}

function main() {
  if (!recordPath || !fs.existsSync(recordPath)) {
    console.log(`FAIL there is no clause record at ${recordPath}`);
    return 1;
  }
  const ran = ledgerPath ? readLedger(ledgerPath).claims : [];
  const text = fs.readFileSync(recordPath, "utf8");

  const rows = new Map();
  let problems = 0;
  for (const line of text.split("\n")) {
    const match = line.match(/^\s*\|\s*(F\d+)\s*\|([^|]*)\|([^|]*)\|\s*$/);
    if (!match) {
      continue;
    }
    const clause = match[1];
    const assertions = match[2]
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    const exemption = match[3].trim();
    if (rows.has(clause)) {
      problems += fail(`${clause} appears in the record more than once`);
      continue;
    }
    rows.set(clause, { assertions, exemption });
  }

  for (const clause of rows.keys()) {
    if (!CLAUSES.includes(clause)) {
      problems += fail(`${clause} is not a clause of the frontend conventions`);
    }
  }

  for (const clause of CLAUSES) {
    const row = rows.get(clause);
    if (!row) {
      problems += fail(`${clause} is absent from ${recordPath}`);
      continue;
    }
    if (row.assertions.length > 0 && row.exemption) {
      problems += fail(
        `${clause} carries both an assertion (${row.assertions.join(", ")}) and an exemption (${row.exemption})`
      );
      continue;
    }
    if (row.assertions.length === 0 && !row.exemption) {
      problems += fail(`${clause} names neither an assertion nor an exemption`);
      continue;
    }
    for (const assertion of row.assertions) {
      if (!Object.prototype.hasOwnProperty.call(CLAIMS, assertion)) {
        problems += fail(
          `${clause} names "${assertion}", which is not a claim this check declares`
        );
        continue;
      }
      if (ledgerPath && !ran.includes(assertion)) {
        problems += fail(
          `${clause} names the assertion "${assertion}", which did not run in this invocation`
        );
      }
    }
  }

  if (problems > 0) {
    return 1;
  }
  console.log(
    `pass all ${CLAUSES.length} clauses are mapped in ${recordPath}, each to one thing, and every assertion named ran`
  );
  return 0;
}

process.exit(main());
