// Minimal stand-in for the backend indexer's /upload-units/by-job-id/:jobId.
// Serves the JobUploadUnitsResponse the TEE's compute expects, sourced from
// docs/data-registry-populate.generated.json. compute only consumes ipfsHash
// (rawCid) + featCid; metaId/unitId are log labels. No Postgres/Helius needed.
const http = require("http");
const fs = require("fs");
const path = require("path");

const PORT = Number(process.env.STUB_PORT || 3000);
const genPath = path.resolve(__dirname, "..", "docs", "data-registry-populate.generated.json");
const gen = JSON.parse(fs.readFileSync(genPath, "utf8"));

// Provider pubkeys for the cohort (selected participants), in metaId order.
const PARTICIPANTS = [
  "49dmB7JEB2iRLQ7LQbebmW8h9RAP5pZ3ywVoyQ421p8s",
  "DkZ1cd883rBYS8Y8AShqgAqBBpRaeWi5By7dEeWSiQ25",
  "8vbjNV1wxij3hZcRuxpgxD3Q4TFfKkjLe1KoAKRZxJu8",
  "Gm6st8pF9NXAaTdVW2ZkaqBPLKryQv3vYCx4y9fwJnie",
  "553eqsKDW3cPe6ZmssJfJmz2vZToUfuKZuDd9j1QGLTx",
];

function buildUnits() {
  const units = [];
  gen.entries.forEach((entry, i) => {
    const metaId = String(i + 1);
    units.push({
      metaId,
      unitId: `${metaId}-0`,
      ipfsHash: entry.rawCid,
      featCid: "",
      dayStartTimestamp: String(entry.dayStartTimestamp),
      dayEndTimestamp: String(entry.dayEndTimestamp),
      date_of_creation: String(entry.dayStartTimestamp),
    });
    (entry.appendedUploads || []).forEach((up, j) => {
      units.push({
        metaId,
        unitId: `${metaId}-${j + 1}`,
        ipfsHash: up.rawCid,
        featCid: "",
        dayStartTimestamp: String(up.dayStartTimestamp),
        dayEndTimestamp: String(up.dayEndTimestamp),
        date_of_creation: String(up.dayStartTimestamp),
      });
    });
  });
  return units;
}

const server = http.createServer((req, res) => {
  const m = req.url.match(/^\/upload-units\/by-job-id\/(\d+)/);
  if (!m) {
    res.writeHead(404, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ error: "not found", url: req.url }));
    return;
  }
  const jobId = m[1];
  const uploadUnits = buildUnits();
  const starts = uploadUnits.map((u) => Number(u.dayStartTimestamp));
  const ends = uploadUnits.map((u) => Number(u.dayEndTimestamp));
  const body = {
    uploadUnits,
    totalCount: uploadUnits.length,
    participants: PARTICIPANTS,
    jobDetails: {
      jobId,
      startDayUtc: Math.min(...starts),
      endDayUtc: Math.max(...ends),
      numDays: 3,
      participantCount: PARTICIPANTS.length,
      metaIdCount: gen.entries.length,
    },
  };
  console.log(`[stub] GET /upload-units/by-job-id/${jobId} -> ${uploadUnits.length} units`);
  res.writeHead(200, { "Content-Type": "application/json" });
  res.end(JSON.stringify(body));
});

server.listen(PORT, "127.0.0.1", () => {
  console.log(`[stub] listening on http://127.0.0.1:${PORT} (units from ${path.basename(genPath)})`);
});
