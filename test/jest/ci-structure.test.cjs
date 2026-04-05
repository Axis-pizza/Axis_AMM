const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const repoRoot = path.resolve(__dirname, "..", "..");

function readFile(relativePath) {
  return fs.readFileSync(path.join(repoRoot, relativePath), "utf8");
}

function runBunScript(relativePath, extraEnv = {}) {
  return spawnSync("bun", [relativePath], {
    cwd: repoRoot,
    env: { ...process.env, ...extraEnv },
    stdio: "inherit",
  });
}

describe("CI Structure", () => {
  test("entry workflow calls reusable workflows to avoid duplicated triggers", () => {
    const entryWorkflow = readFile(".github/workflows/ci.yml");

    expect(entryWorkflow).toContain(".github/workflows/reusable-rust.yml");
    expect(entryWorkflow).toContain(".github/workflows/reusable-typescript.yml");
    expect(entryWorkflow).toContain(".github/workflows/reusable-e2e-local.yml");
    expect(entryWorkflow).toContain("suite: e2e");
    expect(entryWorkflow).toContain("suite: benchmark");
    expect(entryWorkflow).toContain("cancel-in-progress: true");
  });

  test("reusable workflows delegate logic to scripts and step-based e2e suites", () => {
    const rustWorkflow = readFile(".github/workflows/reusable-rust.yml");
    const tsWorkflow = readFile(".github/workflows/reusable-typescript.yml");
    const localE2eWorkflow = readFile(".github/workflows/reusable-e2e-local.yml");

    expect(rustWorkflow).toContain("bash ci/job-rust.sh");
    expect(tsWorkflow).toContain("bash ci/job-typescript.sh");

    expect(localE2eWorkflow).toContain("bash ci/e2e-local-prepare.sh");
    expect(localE2eWorkflow).toContain("suite:");
    expect(localE2eWorkflow).toContain("if: inputs.suite == 'e2e'");
    expect(localE2eWorkflow).toContain("if: inputs.suite == 'benchmark'");
    expect(localE2eWorkflow).toContain("bun run e2e:pfda-amm-legacy:local");
    expect(localE2eWorkflow).toContain("bun run e2e:pfda-amm-3:local");
    expect(localE2eWorkflow).toContain("bun run e2e:axis-g3m:local");
    expect(localE2eWorkflow).toContain("bun run e2e:axis-vault:local");
    expect(localE2eWorkflow).toContain("bun run bench:ab");
    expect(localE2eWorkflow).toContain("bash ci/e2e-local-cleanup.sh");
  });

  test("required CI scripts are tracked", () => {
    const requiredScripts = [
      "ci/setup-rust.sh",
      "ci/install-solana.sh",
      "ci/rust-tests.sh",
      "ci/rust-build-sbf.sh",
      "ci/rust-lint.sh",
      "ci/ts-typecheck.sh",
      "ci/ts-lint.sh",
      "ci/jest.sh",
      "ci/job-rust.sh",
      "ci/job-typescript.sh",
      "ci/job-rust-and-typescript.sh",
      "ci/job-local-e2e.sh",
      "ci/job-local-benchmark.sh",
      "ci/job-devnet-e2e.sh",
      "ci/e2e-local-prepare.sh",
      "ci/e2e-local-cleanup.sh",
      "ci/update-ab-report.sh",
      ".github/workflows/reusable-rust.yml",
      ".github/workflows/reusable-typescript.yml",
      ".github/workflows/reusable-e2e-local.yml",
      ".github/workflows/e2e-devnet.yml",
      ".github/workflows/main-report.yml",
    ];

    for (const script of requiredScripts) {
      expect(fs.existsSync(path.join(repoRoot, script))).toBe(true);
    }
  });

  test("bun runtime is available for test execution", () => {
    const result = spawnSync("bun", ["--version"], {
      cwd: repoRoot,
      encoding: "utf8",
    });

    expect(result.status).toBe(0);
    expect(result.stdout.trim().length).toBeGreaterThan(0);
  });

  const runLocal = process.env.RUN_SOLANA_LOCAL_TESTS === "1" ? test : test.skip;
  runLocal(
    "local suite can be orchestrated by jest through bun",
    () => {
      const result = runBunScript("test/e2e/pfda-amm-3/pfda-amm-3.local.e2e.ts", {
        RPC_URL: "http://localhost:8899",
        WINDOW_SLOTS: "10",
      });

      expect(result.status).toBe(0);
    },
    15 * 60 * 1000,
  );

  const runDevnet = process.env.RUN_SOLANA_DEVNET_TESTS === "1" ? test : test.skip;
  runDevnet(
    "devnet suite can be orchestrated by jest through bun",
    () => {
      const result = runBunScript("test/e2e/pfda-amm-3/pfda-amm-3.o1-proof.test.ts");
      expect(result.status).toBe(0);
    },
    20 * 60 * 1000,
  );
});
