import init, { Session } from "./pkg/flock_audit_report.js";

const CHUNK = 8 * 1024 * 1024;
const $ = (id) => document.getElementById(id);
let session = null;

function save(name, text, type) {
  const url = URL.createObjectURL(new Blob([text], { type }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function run(event) {
  event.preventDefault();
  const files = Array.from($("files").files);
  if (!files.length) return;
  $("go").disabled = true;
  $("actions").hidden = true;
  $("report").innerHTML = "";
  if (session) session.free();
  session = new Session($("home").value);
  const total = files.reduce((n, f) => n + f.size, 0) || 1;
  let done = 0;
  const progress = $("progress");
  progress.hidden = false;
  try {
    for (const file of files) {
      session.begin_file(file.name);
      for (let pos = 0; pos < file.size; pos += CHUNK) {
        const buf = await file.slice(pos, pos + CHUNK).arrayBuffer();
        session.push(new Uint8Array(buf));
        done += buf.byteLength;
        progress.value = done / total;
        $("status").textContent = `Reading ${file.name}… ${Math.round((100 * done) / total)}%`;
      }
      session.end_file();
    }
    $("report").innerHTML = session.report_html();
    $("status").textContent = `Done: ${files.length} file(s) read.`;
    $("actions").hidden = false;
  } catch (e) {
    $("status").textContent = `Could not read the files: ${e}`;
  } finally {
    progress.hidden = true;
    $("go").disabled = false;
  }
}

async function main() {
  await init();
  $("go").disabled = false;
  $("form").addEventListener("submit", run);
  $("copy").addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(session.report_text());
      $("status").textContent = "Report copied as text.";
    } catch {
      $("status").textContent = "Copy is not allowed here; use Save report instead.";
    }
  });
  $("save-text").addEventListener("click", () =>
    save("flock-audit-report.txt", session.report_text(), "text/plain"));
  $("save-csv").addEventListener("click", () =>
    save("flock-agencies.csv", session.agencies_csv(), "text/csv"));
  $("print").addEventListener("click", () => window.print());
}

main().catch((e) => {
  $("status").textContent = `This browser could not start the page: ${e}`;
});
