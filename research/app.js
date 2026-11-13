/* Zeus Technical Report TR-01 & Systems Workbench
 * All scheduling, scope-isolation, UCB1, and Judge logic mirrors src/{runtime,policy,judge,output}.rs.
 */

const REPORT_META = {
  baselineCommit: '0692fa6+',
  modules: 18,
  loc: 7852,
  tests: 269,
  suites: 48,
};

const EMBEDDED_FIELD_RUNS = {
  description:
    'Scrubbed field trials of Zeus on the public kv.py persistence-repair fixture with a local 9B model. Exploratory engineering runs, not a held-out benchmark.',
  source: 'research/scripts/extract_field_runs.py over local retained runs',
  trials: [
    { trial: 'T01', campaign: 'smoke', started_at: '2026-10-31T18:43:26+00:00', approx_wall_seconds: 43, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'infrastructure_error', failure_class: 'path-contract', addressed_by: null, error: 'only relative normal path components allowed', baseline_failures: 9, verifier_checks: null, verifier_failures: null, tool_requests: 1, parallel_workers: 0, planned: false, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T02', campaign: 'smoke', started_at: '2026-10-31T18:46:34+00:00', approx_wall_seconds: 52, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'infrastructure_error', failure_class: 'transport', addressed_by: '3267d6f', error: 'model request failed: Io', baseline_failures: 9, verifier_checks: null, verifier_failures: null, tool_requests: 1, parallel_workers: 0, planned: false, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T03', campaign: 'smoke', started_at: '2026-10-31T19:02:17+00:00', approx_wall_seconds: 334, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'incomplete', failure_class: null, addressed_by: null, error: null, baseline_failures: 9, verifier_checks: null, verifier_failures: null, tool_requests: 8, parallel_workers: 0, planned: false, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T04', campaign: 'model-test', started_at: '2026-10-31T19:09:44+00:00', approx_wall_seconds: 463, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'verified', failure_class: null, addressed_by: null, error: null, baseline_failures: 9, verifier_checks: 77, verifier_failures: 0, tool_requests: 8, parallel_workers: 0, planned: false, reward_recorded: true, policy_updated_in_report: false },
    { trial: 'T05', campaign: 'acceptance', started_at: '2026-11-01T05:17:01+00:00', approx_wall_seconds: 165, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'infrastructure_error', failure_class: 'transport', addressed_by: '3267d6f', error: 'model request failed: Io', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 5, parallel_workers: 0, planned: false, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T06', campaign: 'acceptance', started_at: '2026-11-01T05:23:07+00:00', approx_wall_seconds: 250, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'verified', failure_class: null, addressed_by: null, error: null, baseline_failures: 11, verifier_checks: 82, verifier_failures: 0, tool_requests: 4, parallel_workers: 0, planned: false, reward_recorded: true, policy_updated_in_report: false },
    { trial: 'T07', campaign: 'acceptance', started_at: '2026-11-01T05:27:26+00:00', approx_wall_seconds: 130, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'infrastructure_error', failure_class: 'planner-format', addressed_by: 'd121e6b', error: 'planner must return JSON without fences', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 0, parallel_workers: 0, planned: true, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T08', campaign: 'acceptance', started_at: '2026-11-01T10:34:11+00:00', approx_wall_seconds: 517, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'infrastructure_error', failure_class: 'transport', addressed_by: '3267d6f', error: 'model request failed: Io', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 7, parallel_workers: 0, planned: true, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T09', campaign: 'acceptance', started_at: '2026-11-01T12:11:41+00:00', approx_wall_seconds: 200, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'infrastructure_error', failure_class: 'planner-format', addressed_by: 'd121e6b', error: 'planner must return a JSON object with only tasks', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 2, parallel_workers: 0, planned: true, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T10', campaign: 'acceptance', started_at: '2026-11-01T12:15:55+00:00', approx_wall_seconds: 47, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'infrastructure_error', failure_class: 'planner-format', addressed_by: 'd121e6b', error: 'planner must return a JSON object with only tasks', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 0, parallel_workers: 0, planned: true, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T11', campaign: 'acceptance', started_at: '2026-11-01T12:18:13+00:00', approx_wall_seconds: 335, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'infrastructure_error', failure_class: 'capability-request', addressed_by: '3f7e00a', error: 'unknown tool requested', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 8, parallel_workers: 0, planned: true, reward_recorded: false, policy_updated_in_report: false },
    { trial: 'T12', campaign: 'acceptance', started_at: '2026-11-01T12:34:12+00:00', approx_wall_seconds: 638, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'verified', failure_class: null, addressed_by: null, error: null, baseline_failures: 11, verifier_checks: 82, verifier_failures: 0, tool_requests: 8, parallel_workers: 2, planned: true, reward_recorded: true, policy_updated_in_report: false },
    { trial: 'T13', campaign: 'router-training', started_at: '2026-11-03T03:37:02+00:00', approx_wall_seconds: 225, model: 'qwen/qwen3.5-9b', strategy: 'direct', status: 'verified', failure_class: null, addressed_by: null, error: null, baseline_failures: 11, verifier_checks: 82, verifier_failures: 0, tool_requests: 5, parallel_workers: 0, planned: false, reward_recorded: true, policy_updated_in_report: false },
    { trial: 'T14', campaign: 'router-training', started_at: '2026-11-03T03:40:46+00:00', approx_wall_seconds: 986, model: 'qwen/qwen3.5-9b', strategy: 'parallel', status: 'infrastructure_error', failure_class: 'transport', addressed_by: '3267d6f', error: 'model request failed: Io', baseline_failures: 11, verifier_checks: null, verifier_failures: null, tool_requests: 8, parallel_workers: 2, planned: true, reward_recorded: false, policy_updated_in_report: false },
  ],
};

const FIGURE_PLATES = [
  {
    id: 'fig1',
    label: 'Fig 1 · Architecture',
    src: '../docs/figures/fig1-system.svg',
    title: 'Dual-Plane Control Architecture & Trust Boundaries',
    caption: 'Separates probabilistic model inference (untrusted planner/workers) from deterministic Rust host guards: candidate snapshot, pre-admission scope graph, sandboxed verifier, UCB1 policy, and atomic JSON state.',
  },
  {
    id: 'fig2',
    label: 'Fig 2 · FIFO vs Barrier',
    src: '../docs/figures/fig2-scheduler.svg',
    title: 'Work-Conserving FIFO Queue vs Synchronous Batch Barriers',
    caption: 'On skewed task durations, bounded FIFO dispatch (k=3) eliminates wave-barrier idle gaps and halts queued admissions immediately when a running worker fails.',
  },
  {
    id: 'fig3',
    label: 'Fig 3 · Ownership Graph',
    src: '../docs/figures/fig3-ownership.svg',
    title: 'Theorem 1: Pre-Admission Write-Scope Isolation',
    caption: 'Before any parallel coder spawns, Zeus validates ASCII canonical paths, user autonomy allowlists, and case-folded disjoint ancestor prefixes, while enforcing <=8,192 B handoff bounds.',
  },
  {
    id: 'fig4',
    label: 'Fig 4 · UCB1 Routing',
    src: '../docs/figures/fig4-routing.svg',
    title: 'Infrastructure-Isolated UCB1 Routing Policy',
    caption: 'Only binary verifier outcomes update UCB1 arm statistics (trials, successes); transport errors, timeouts, and planner format rejections leave policy weights strictly untouched.',
  },
  {
    id: 'fig5',
    label: 'Fig 5 · Field Trials',
    src: '../docs/figures/fig5-field-trials.svg',
    title: 'Chronological Breakdown of Exploratory kv.py Trials (T01–T14)',
    caption: 'Tracks 14 real Qwen 3.5-9B local runs from early path/transport errors through planner-fence & capability hardenings to verified direct and parallel runs.',
  },
  {
    id: 'fig6',
    label: 'Fig 6 · Verification Matrix',
    src: '../docs/figures/fig6-tests.svg',
    title: 'Deterministic Rust Verification Coverage Across 45 Test Suites',
    caption: 'Maps deterministic unit and integration test suites to safety, sandbox, budget, MCP, auth, routing, and shadow-judge invariants without live API dependencies.',
  },
  {
    id: 'fig7',
    label: 'Fig 7 · Judge & Handoffs',
    src: '../docs/figures/fig7-judge-handoffs.svg',
    title: 'Zeus Judge 8-Gate Abstention Contract & Head/Tail Handoff Slice',
    caption: 'Illustrates the fail-closed shadow scorer gates (no_model through ambiguous) alongside the 75% head / 25% tail 8,192-byte worker_handoff budget.',
  },
  {
    id: 'cover',
    label: 'TR-01 Cover Plate',
    src: '../docs/figures/zeus-cover.svg',
    title: 'Zeus Technical Report TR-01 Cover Diagram',
    caption: 'Archival monograph title plate summarizing Zeus architecture, verification suites, and bounded agent guarantees.',
  },
];

function escapeHtml(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function utf8Bytes(str) {
  return new TextEncoder().encode(str).length;
}

/* ── 1. FIFO Scheduler Simulator (<FifoSchedulerLab>) ─────────────────────── */

const SCHED_PRESETS = {
  skewed: { k: 3, failHalts: false, failTask: 2, durations: '9, 2, 3, 2, 7, 1, 4' },
  uniform: { k: 3, failHalts: false, failTask: 2, durations: '4, 4, 4, 4, 4, 4' },
  tail: { k: 3, failHalts: false, failTask: 2, durations: '1, 1, 1, 1, 10, 2, 2' },
  failstop: { k: 3, failHalts: true, failTask: 2, durations: '6, 2, 5, 4, 3, 4, 2' },
};

function parseDurations(raw) {
  const trimmed = raw.trim();
  if (!trimmed) {
    return { error: 'Enter 1 to 12 comma-separated positive durations (for example: 9, 2, 3, 2, 7, 1, 4).' };
  }
  const parts = trimmed.split(',').map((s) => s.trim()).filter(Boolean);
  if (parts.length < 1 || parts.length > 12) {
    return { error: `Expected 1..12 coding tasks (found ${parts.length}). Zeus bounds coding plans at 12 workers.` };
  }
  const nums = [];
  for (const part of parts) {
    const val = Number(part);
    if (!Number.isFinite(val) || val <= 0 || val > 60) {
      return { error: `Invalid duration "${part}". Use positive numbers between 0.5 and 60.` };
    }
    nums.push(Math.round(val * 10) / 10);
  }
  return { tasks: nums };
}

function simulateFifo(durations, k, failHalts, failIdx) {
  const n = durations.length;
  const active = [];
  let nextIdx = 0;
  let now = 0;
  let halted = false;
  const records = durations.map((d, idx) => ({
    idx,
    duration: d,
    lane: null,
    start: null,
    end: null,
    status: 'queued',
  }));

  const freeLanes = Array.from({ length: k }, (_, i) => i);

  while (nextIdx < n || active.length > 0) {
    while (!halted && nextIdx < n && freeLanes.length > 0) {
      const lane = freeLanes.shift();
      const rec = records[nextIdx];
      rec.lane = lane;
      rec.start = now;
      rec.end = now + rec.duration;
      rec.status = failHalts && nextIdx === failIdx ? 'failed' : 'ok';
      active.push({ idx: nextIdx, lane, end: rec.end, willFail: failHalts && nextIdx === failIdx });
      nextIdx += 1;
    }
    if (active.length === 0) break;
    active.sort((a, b) => a.end - b.end);
    const soonest = active[0].end;
    now = soonest;
    const finished = [];
    for (let i = active.length - 1; i >= 0; i -= 1) {
      if (Math.abs(active[i].end - soonest) < 1e-9) {
        finished.push(active.splice(i, 1)[0]);
      }
    }
    finished.sort((a, b) => a.lane - b.lane);
    for (const item of finished) {
      freeLanes.push(item.lane);
      if (item.willFail) {
        halted = true;
      }
    }
    freeLanes.sort((a, b) => a - b);
    if (halted && active.length === 0) break;
  }

  for (const rec of records) {
    if (rec.start === null) {
      rec.status = 'cancelled-in-queue';
    }
  }

  const makespan = Math.max(0, ...records.filter((r) => r.end !== null).map((r) => r.end));
  const executedBusy = records.filter((r) => r.end !== null).reduce((s, r) => s + r.duration, 0);
  const idle = Math.max(0, makespan * k - executedBusy);
  const dropped = records.filter((r) => r.status === 'cancelled-in-queue').length;
  return { records, makespan, idle, dropped };
}

function simulateBarrier(durations, k, failHalts, failIdx) {
  const n = durations.length;
  const records = durations.map((d, idx) => ({
    idx,
    duration: d,
    lane: null,
    start: null,
    end: null,
    status: 'queued',
  }));
  const barriers = [];
  let now = 0;
  let halted = false;
  for (let base = 0; base < n && !halted; base += k) {
    const wave = durations.slice(base, base + k);
    const waveMax = Math.max(...wave);
    for (let offset = 0; offset < wave.length; offset += 1) {
      const idx = base + offset;
      records[idx].lane = offset;
      records[idx].start = now;
      records[idx].end = now + durations[idx];
      records[idx].status = failHalts && idx === failIdx ? 'failed' : 'ok';
      if (failHalts && idx === failIdx) {
        halted = true;
      }
    }
    now += waveMax;
    barriers.push(now);
  }
  for (const rec of records) {
    if (rec.start === null) {
      rec.status = 'cancelled-in-queue';
    }
  }
  const makespan = now;
  const executedBusy = records.filter((r) => r.end !== null).reduce((s, r) => s + r.duration, 0);
  const idle = Math.max(0, makespan * k - executedBusy);
  const dropped = records.filter((r) => r.status === 'cancelled-in-queue').length;
  return { records, makespan, idle, dropped, barriers };
}

function renderGantt(container, result, k, maxTime, barriers = []) {
  const scale = maxTime > 0 ? maxTime : 1;
  let html = '<div class="gantt-lanes">';
  for (let lane = 0; lane < k; lane += 1) {
    const bars = result.records
      .filter((r) => r.lane === lane)
      .map((r) => {
        const left = ((r.start / scale) * 100).toFixed(2);
        const width = (((r.end - r.start) / scale) * 100).toFixed(2);
        const cls = r.status === 'failed' ? 'gantt-bar gantt-bar-fail' : 'gantt-bar';
        return `<div class="${cls}" style="left:${left}%;width:${width}%" title="T${r.idx + 1}: ${r.start}..${r.end} (${r.duration}u)">T${r.idx + 1} (${r.duration}u)</div>`;
      })
      .join('');
    const markers = barriers
      .map((b) => `<div class="gantt-bar-barrier" style="left:${((b / scale) * 100).toFixed(2)}%" title="Wave barrier @ ${b}u"></div>`)
      .join('');
    html += `
      <div class="gantt-lane">
        <span class="lane-label">Worker #${lane + 1}</span>
        <div class="lane-track">${bars}${markers}</div>
      </div>`;
  }
  const unstarted = result.records.filter((r) => r.status === 'cancelled-in-queue');
  if (unstarted.length > 0) {
    html += `<p class="control-hint" style="margin:0.35rem 0 0 82px;">Admitted queue halted on failure; never dispatched: ${unstarted
      .map((u) => `T${u.idx + 1}`)
      .join(', ')}</p>`;
  }
  html += `<div class="gantt-axis"><span>0u</span><span>${(scale / 2).toFixed(1)}u</span><span>${scale.toFixed(1)}u</span></div></div>`;
  container.innerHTML = html;
}

function initSchedulerLab() {
  const kButtons = document.querySelectorAll('[data-sched-k]');
  const presetButtons = document.querySelectorAll('[data-sched-preset]');
  const durationsInput = document.getElementById('sched-durations');
  const failCheck = document.getElementById('sched-fail-check');
  const failIndexInput = document.getElementById('sched-fail-index');
  const errorBox = document.getElementById('sched-error');
  const fifoMakespanEl = document.getElementById('fifo-makespan');
  const barrierMakespanEl = document.getElementById('barrier-makespan');
  const schedDeltaEl = document.getElementById('sched-delta');
  const grahamBoundEl = document.getElementById('graham-bound');
  const fifoBadgeEl = document.getElementById('fifo-badge');
  const barrierBadgeEl = document.getElementById('barrier-badge');
  const fifoGanttEl = document.getElementById('fifo-gantt');
  const barrierGanttEl = document.getElementById('barrier-gantt');
  const liveEl = document.getElementById('sched-live');

  let currentK = 3;

  function render() {
    const parsed = parseDurations(durationsInput.value);
    if (parsed.error) {
      errorBox.hidden = false;
      errorBox.textContent = parsed.error;
      fifoGanttEl.innerHTML = '<div class="empty-state">Adjust task durations above to render the FIFO vs Barrier schedules.</div>';
      barrierGanttEl.innerHTML = '';
      fifoMakespanEl.textContent = '—';
      barrierMakespanEl.textContent = '—';
      schedDeltaEl.textContent = '—';
      grahamBoundEl.textContent = '—';
      liveEl.textContent = parsed.error;
      return;
    }
    errorBox.hidden = true;
    const tasks = parsed.tasks;
    const maxIdx = tasks.length;
    failIndexInput.max = String(maxIdx);
    let failIdx = Math.max(1, Math.min(maxIdx, Number(failIndexInput.value) || 2)) - 1;
    failIndexInput.value = String(failIdx + 1);
    const failHalts = failCheck.checked;
    failIndexInput.disabled = !failHalts;

    const fifo = simulateFifo(tasks, currentK, failHalts, failIdx);
    const barrier = simulateBarrier(tasks, currentK, failHalts, failIdx);
    const axisMax = Math.max(fifo.makespan, barrier.makespan, 1);

    renderGantt(fifoGanttEl, fifo, currentK, axisMax);
    renderGantt(barrierGanttEl, barrier, currentK, axisMax, barrier.barriers);

    const totalWork = tasks.reduce((a, b) => a + b, 0);
    const maxTask = Math.max(...tasks);
    const graham = maxTask * (1 - 1 / currentK) + totalWork / currentK;

    fifoMakespanEl.textContent = `${fifo.makespan.toFixed(1)}u`;
    barrierMakespanEl.textContent = `${barrier.makespan.toFixed(1)}u`;
    const saved = barrier.makespan - fifo.makespan;
    const pct = barrier.makespan > 0 ? ((saved / barrier.makespan) * 100).toFixed(1) : '0.0';
    schedDeltaEl.textContent = saved >= 0 ? `-${saved.toFixed(1)}u (${pct}%)` : `+${Math.abs(saved).toFixed(1)}u`;
    grahamBoundEl.textContent = `${graham.toFixed(1)}u`;

    fifoBadgeEl.textContent = `Idle ${fifo.idle.toFixed(1)}u${fifo.dropped ? ` · ${fifo.dropped} halted` : ''}`;
    barrierBadgeEl.textContent = `Idle ${barrier.idle.toFixed(1)}u${barrier.dropped ? ` · ${barrier.dropped} skipped` : ''}`;
    liveEl.textContent = `FIFO makespan ${fifo.makespan.toFixed(1)}u versus batch barrier ${barrier.makespan.toFixed(1)}u on k=${currentK} workers.`;
  }

  kButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      currentK = Number(btn.dataset.schedK);
      kButtons.forEach((b) => b.setAttribute('aria-pressed', String(b === btn)));
      render();
    });
  });

  presetButtons.forEach((btn) => {
    btn.addEventListener('click', () => {
      const preset = SCHED_PRESETS[btn.dataset.schedPreset];
      if (!preset) return;
      presetButtons.forEach((b) => b.setAttribute('aria-pressed', String(b === btn)));
      currentK = preset.k;
      kButtons.forEach((b) => b.setAttribute('aria-pressed', String(Number(b.dataset.schedK) === currentK)));
      durationsInput.value = preset.durations;
      failCheck.checked = preset.failHalts;
      failIndexInput.value = String(preset.failTask);
      render();
    });
  });

  durationsInput.addEventListener('input', render);
  failCheck.addEventListener('change', render);
  failIndexInput.addEventListener('input', render);
  render();
}

/* ── 2. Pre-Admission Scope Validator (<ScopeValidatorLab>) ───────────────── */

function makeLongText(bytesTarget) {
  const unit = 'verified invariant note; ';
  return unit.repeat(Math.ceil(bytesTarget / unit.length)).slice(0, bytesTarget);
}

const SCOPE_PRESETS = {
  disjoint: {
    allowlist: 'src/a.rs\nsrc/b.rs\nsrc/c.rs',
    plan: JSON.stringify(
      {
        tasks: [
          { task: 'Add parser validation', write_paths: ['src/a.rs'], context: 'Line 14..42 inspected' },
          { task: 'Update formatter helper', write_paths: ['src/b.rs'] },
          { task: 'Add unit tests for parser', write_paths: ['src/c.rs'] },
        ],
      },
      null,
      2,
    ),
  },
  caseAlias: {
    allowlist: '',
    plan: JSON.stringify(
      {
        tasks: [
          { task: 'Refactor parser module', write_paths: ['src/Parser.rs'] },
          { task: 'Normalize error formatting', write_paths: ['src/parser.rs'] },
        ],
      },
      null,
      2,
    ),
  },
  ancestor: {
    allowlist: '',
    plan: JSON.stringify(
      {
        tasks: [
          { task: 'Touch directory prefix', write_paths: ['src'] },
          { task: 'Edit module inside src', write_paths: ['src/lib.rs'] },
        ],
      },
      null,
      2,
    ),
  },
  traversal: {
    allowlist: 'src/lib.rs',
    plan: JSON.stringify(
      {
        tasks: [
          { task: 'Read and patch src/lib.rs', write_paths: ['src/lib.rs'] },
          { task: 'Escape candidate directory', write_paths: ['../secrets.txt'] },
        ],
      },
      null,
      2,
    ),
  },
  handoff: {
    allowlist: '',
    plan: JSON.stringify(
      {
        tasks: [
          { task: 'Patch kv parser', write_paths: ['src/kv.rs'], context: makeLongText(8500) },
          { task: 'Update kv tests', write_paths: ['tests/kv_test.rs'] },
        ],
      },
      null,
      2,
    ),
  },
};

function validateScopePlan(planText, allowlistText) {
  const checks = [];
  if (!planText.trim()) {
    return {
      ok: false,
      verdict: 'Empty planner payload — waiting for JSON plan object with {"tasks": [...]}.',
      checks: [
        {
          name: '1. Envelope & schema (<= 64 KiB)',
          detail: 'planner input is empty',
          pass: false,
        },
      ],
      maxContextBytes: 0,
      workerCount: 0,
    };
  }

  let parsed;
  try {
    parsed = JSON.parse(planText);
  } catch (err) {
    return {
      ok: false,
      verdict: `Syntax error — planner output is rejected fail-closed: ${err.message}`,
      checks: [
        {
          name: '1. Envelope & JSON decode',
          detail: err.message,
          pass: false,
        },
      ],
      maxContextBytes: 0,
      workerCount: 0,
    };
  }

  const envelopeBytes = utf8Bytes(planText);
  const topKeys = parsed && typeof parsed === 'object' && !Array.isArray(parsed) ? Object.keys(parsed) : null;
  const schemaOk = Boolean(topKeys && topKeys.length === 1 && topKeys[0] === 'tasks' && Array.isArray(parsed.tasks) && envelopeBytes <= 65536);

  if (!schemaOk) {
    checks.push({
      name: '1. Envelope & schema (<= 64 KiB)',
      detail: 'planner must return a JSON object with only tasks (<= 64 KiB)',
      pass: false,
    });
    return {
      ok: false,
      verdict: 'Rejected at schema deserialization: planner must return a JSON object with only "tasks".',
      checks,
      maxContextBytes: 0,
      workerCount: 0,
    };
  }

  const tasks = parsed.tasks;
  const allCoders = tasks.every((t) => t && typeof t === 'object' && !Array.isArray(t));
  const maxTasks = allCoders ? 12 : 3;
  const countOk = tasks.length >= 2 && tasks.length <= maxTasks;
  checks.push({
    name: `1. Envelope & cardinality (2..=${maxTasks})`,
    detail: countOk
      ? `${tasks.length} ${allCoders ? 'coder' : 'analyst/mixed'} tasks, ${envelopeBytes} B`
      : `planner must produce 2..${maxTasks} tasks (got ${tasks.length})`,
    pass: countOk,
  });

  const allowlistRaw = allowlistText.trim();
  const hasAllowlist = allowlistRaw.length > 0;
  const allowlist = hasAllowlist
    ? allowlistRaw
        .split(/[\n,]+/)
        .map((s) => s.trim())
        .filter(Boolean)
    : null;

  let pathOk = true;
  let pathDetail = 'All write_paths are non-empty ASCII relative exact files';
  let allowOk = true;
  let allowDetail = hasAllowlist
    ? `All paths contained in autonomy.write_paths (${allowlist.length} entries)`
    : 'No explicit autonomy.write_paths cap configured';
  let disjointOk = true;
  let disjointDetail = 'All lowercased paths are pairwise disjoint without ancestor/descendant overlap';
  let contextOk = true;
  let maxContextBytes = 0;

  const owned = [];
  for (let i = 0; i < tasks.length; i += 1) {
    const entry = tasks[i];
    if (typeof entry === 'string') {
      if (!entry.trim() || utf8Bytes(entry) > 4096) {
        pathOk = false;
        pathDetail = `Task #${i + 1}: invalid plan task string`;
      }
      continue;
    }
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      pathOk = false;
      pathDetail = `Task #${i + 1}: invalid task entry type`;
      continue;
    }
    const keys = Object.keys(entry);
    const badKey = keys.find((k) => !['task', 'write_paths', 'context'].includes(k));
    if (badKey || typeof entry.task !== 'string' || !entry.task.trim() || utf8Bytes(entry.task) > 4096) {
      pathOk = false;
      pathDetail = badKey ? `Task #${i + 1}: unknown field "${badKey}"` : `Task #${i + 1}: invalid plan task`;
    }
    if (entry.context !== undefined) {
      if (typeof entry.context !== 'string') {
        contextOk = false;
      } else {
        const bytes = utf8Bytes(entry.context);
        maxContextBytes = Math.max(maxContextBytes, bytes);
        if (bytes > 8192) contextOk = false;
      }
    }
    if (!Array.isArray(entry.write_paths) || entry.write_paths.length === 0) {
      pathOk = false;
      pathDetail = `Task #${i + 1}: planner write_paths must not be empty`;
      continue;
    }
    for (const p of entry.write_paths) {
      if (typeof p !== 'string' || !p) {
        pathOk = false;
        pathDetail = `Task #${i + 1}: non-string or empty write_path`;
        continue;
      }
      const isAscii = /^[\x00-\x7F]*$/.test(p);
      const hasControl = /[\x00-\x1F\x7F]/.test(p);
      const badSegs = p.split('/').some((seg) => !seg || seg === '.' || seg === '..');
      if (!isAscii || p.includes('\\') || p.includes(':') || hasControl || badSegs) {
        pathOk = false;
        pathDetail = `Rejected "${p}": planner write_paths must be canonical ASCII relative exact files`;
      }
      if (allowlist && !allowlist.includes(p)) {
        allowOk = false;
        allowDetail = `Rejected "${p}": planner write_paths exceeds user autonomy scope`;
      }
      const lower = p.toLowerCase();
      const collision = owned.find(
        (prior) => prior === lower || lower.startsWith(`${prior}/`) || prior.startsWith(`${lower}/`),
      );
      if (collision) {
        disjointOk = false;
        disjointDetail = `Collision between "${collision}" and "${lower}": planner write_paths overlap or duplicate`;
      }
      owned.push(lower);
    }
  }

  checks.push({ name: '2. Canonical ASCII exact files', detail: pathDetail, pass: pathOk });
  checks.push({ name: '3. User autonomy.write_paths subset', detail: allowDetail, pass: allowOk });
  checks.push({ name: '4. Theorem 1 disjoint & ancestor-free', detail: disjointDetail, pass: disjointOk });
  checks.push({
    name: '5. Context & handoff <= 8,192 B',
    detail: contextOk
      ? `Max worker context: ${maxContextBytes} / 8,192 B`
      : `Rejected (${maxContextBytes} B): planner context exceeds 8192 UTF-8 bytes`,
    pass: contextOk,
  });

  const allPass = checks.every((c) => c.pass);
  return {
    ok: allPass,
    verdict: allPass
      ? `ADMITTED — Authority graph valid for ${tasks.length} isolated workers (${owned.length} write paths).`
      : 'REJECTED FAIL-CLOSED — Zero workers spawned; no candidate mutations allowed.',
    checks,
    maxContextBytes,
    workerCount: tasks.length,
  };
}

function initScopeLab() {
  const allowEl = document.getElementById('scope-allowlist');
  const planEl = document.getElementById('scope-json');
  const verdictEl = document.getElementById('scope-verdict');
  const listEl = document.getElementById('scope-checks');
  const handoffBytesEl = document.getElementById('handoff-bytes');
  const handoffFillEl = document.getElementById('handoff-fill');
  const handoffPreviewEl = document.getElementById('handoff-preview');
  const presetBtns = document.querySelectorAll('[data-scope-preset]');

  function render() {
    const res = validateScopePlan(planEl.value, allowEl.value);
    verdictEl.className = `status-banner ${res.ok ? 'status-banner-pass' : 'status-banner-fail'}`;
    verdictEl.textContent = res.verdict;

    listEl.innerHTML = res.checks
      .map(
        (c) => `
        <div class="invariant-item">
          <div>
            <strong>${escapeHtml(c.name)}</strong>
            <small>${escapeHtml(c.detail)}</small>
          </div>
          <span class="pill ${c.pass ? 'pill-pass' : 'pill-fail'}">${c.pass ? 'PASS' : 'REJECT'}</span>
        </div>`,
      )
      .join('');

    const bytes = res.maxContextBytes;
    const pct = Math.min(100, (bytes / 8192) * 100);
    handoffBytesEl.textContent = `${bytes.toLocaleString()} / 8,192 UTF-8 B`;
    handoffFillEl.style.width = `${pct.toFixed(1)}%`;
    handoffFillEl.className = bytes > 8192 ? 'meter-fill over' : 'meter-fill';
    handoffPreviewEl.textContent =
      bytes > 8192
        ? 'Planner admission fails >8,192 B. On worker outputs exceeding 8,192 B, Zeus retains 75% head (~6 KiB) + "[zeus: middle omitted]" + 25% tail (~2 KiB).'
        : 'Within 8,192-byte context budget. Worker handoffs use identical 8,192-byte bounds via worker_handoff.';
  }

  presetBtns.forEach((btn) => {
    btn.addEventListener('click', () => {
      const p = SCOPE_PRESETS[btn.dataset.scopePreset];
      if (!p) return;
      presetBtns.forEach((b) => b.setAttribute('aria-pressed', String(b === btn)));
      allowEl.value = p.allowlist;
      planEl.value = p.plan;
      render();
    });
  });

  allowEl.addEventListener('input', render);
  planEl.addEventListener('input', render);
  render();
}

/* ── 3. UCB1 & Zeus Judge Sandbox (<RouterJudgeSandbox>) ────────────────── */

function ucbDecision(state) {
  if (state.direct.trials === 0) {
    return { next: 'Direct', reason: 'Direct is untried (trials == 0): explore Direct first.', directScore: null, parallelScore: null };
  }
  if (state.parallel.trials === 0) {
    return { next: 'Parallel', reason: 'Parallel is untried (trials == 0): explore Parallel second.', directScore: state.direct.successes / state.direct.trials, parallelScore: null };
  }
  const total = state.direct.trials + state.parallel.trials;
  const dMean = state.direct.successes / state.direct.trials;
  const pMean = state.parallel.successes / state.parallel.trials;
  const dScore = dMean + Math.sqrt((2 * Math.log(total)) / state.direct.trials);
  const pScore = pMean + Math.sqrt((2 * Math.log(total)) / state.parallel.trials);
  const next = pScore > dScore ? 'Parallel' : 'Direct';
  return {
    next,
    reason: pScore > dScore ? 'Parallel UCB exceeds Direct UCB.' : Math.abs(pScore - dScore) < 1e-12 ? 'Exact UCB tie breaks toward simpler Direct.' : 'Direct UCB >= Parallel UCB.',
    directScore: dScore,
    parallelScore: pScore,
  };
}

function initRouterAndJudgeLab() {
  const ucbState = {
    direct: { trials: 1, successes: 1 },
    parallel: { trials: 1, successes: 1 },
    events: ['Loaded Acceptance Campaign baseline (Direct 1/1, Parallel 1/1; 6 infra errors skipped).'],
  };

  const dCard = document.getElementById('ucb-card-direct');
  const pCard = document.getElementById('ucb-card-parallel');
  const dStats = document.getElementById('ucb-stats-direct');
  const pStats = document.getElementById('ucb-stats-parallel');
  const dScoreEl = document.getElementById('ucb-score-direct');
  const pScoreEl = document.getElementById('ucb-score-parallel');
  const ucbNextEl = document.getElementById('ucb-next-banner');
  const ucbLogEl = document.getElementById('ucb-log');

  function renderUcb() {
    const decision = ucbDecision(ucbState);
    dCard.className = `arm-card ${decision.next === 'Direct' ? 'selected' : ''}`;
    pCard.className = `arm-card ${decision.next === 'Parallel' ? 'selected' : ''}`;
    dStats.textContent = `trials=${ucbState.direct.trials}, successes=${ucbState.direct.successes}`;
    pStats.textContent = `trials=${ucbState.parallel.trials}, successes=${ucbState.parallel.successes}`;
    dScoreEl.textContent = decision.directScore === null ? 'UNTRIED' : decision.directScore.toFixed(4);
    pScoreEl.textContent = decision.parallelScore === null ? 'UNTRIED' : decision.parallelScore.toFixed(4);
    ucbNextEl.textContent = `Next UCB1 Choice: ${decision.next} — ${decision.reason}`;
    ucbLogEl.textContent = ucbState.events.slice(-6).join('\n');
  }

  document.getElementById('ucb-btn-pass').addEventListener('click', () => {
    const pick = ucbDecision(ucbState).next;
    const arm = pick === 'Direct' ? ucbState.direct : ucbState.parallel;
    arm.trials += 1;
    arm.successes += 1;
    ucbState.events.push(`[verified] arm=${pick} -> reward=1 (trials=${arm.trials}, successes=${arm.successes})`);
    renderUcb();
  });

  document.getElementById('ucb-btn-fail').addEventListener('click', () => {
    const pick = ucbDecision(ucbState).next;
    const arm = pick === 'Direct' ? ucbState.direct : ucbState.parallel;
    arm.trials += 1;
    ucbState.events.push(`[failed] arm=${pick} -> reward=0 (trials=${arm.trials}, successes=${arm.successes})`);
    renderUcb();
  });

  document.getElementById('ucb-btn-infra').addEventListener('click', () => {
    const pick = ucbDecision(ucbState).next;
    ucbState.events.push(`[infrastructure_error] arm=${pick} -> reward_recorded=false (0 policy change!)`);
    renderUcb();
  });

  document.getElementById('ucb-btn-reset').addEventListener('click', () => {
    ucbState.direct = { trials: 0, successes: 0 };
    ucbState.parallel = { trials: 0, successes: 0 };
    ucbState.events = ['Policy reset (0/0 Direct, 0/0 Parallel). Untried Direct explores first.'];
    renderUcb();
  });

  // Zeus Judge 8-gate sandbox
  const judgeInputs = {
    modelPresent: document.getElementById('j-model-present'),
    validSchema: document.getElementById('j-valid-schema'),
    domainMatch: document.getElementById('j-domain-match'),
    verifier: document.getElementById('j-verifier'),
    samples: document.getElementById('j-samples'),
    taskBytes: document.getElementById('j-task-bytes'),
    maxTurns: document.getElementById('j-turns'),
    maxRepairs: document.getElementById('j-repairs'),
    costBounds: document.getElementById('j-cost-bounds'),
    logitGap: document.getElementById('j-gap'),
  };

  const judgeDecisionEl = document.getElementById('judge-decision');
  const judgeProbsEl = document.getElementById('judge-probs');
  const judgeGatesEl = document.getElementById('judge-gates');
  const judgeFeatEl = document.getElementById('judge-features');

  function sigmoid(z) {
    return 1 / (1 + Math.exp(-z));
  }

  function renderJudge() {
    const tb = Number(judgeInputs.taskBytes.value) || 0;
    const mt = Number(judgeInputs.maxTurns.value) || 0;
    const mr = Number(judgeInputs.maxRepairs.value) || 0;
    const features = [
      tb / 32768,
      mt / 32,
      mr / 8,
      64 / 1024,
      1,
      Math.log1p(131072) / Math.log1p(Number.MAX_SAFE_INTEGER),
      1,
      1800 / 86400,
    ];
    const inSupport = features[0] >= 0 && features[0] <= 1 && features[1] >= 0 && features[1] <= 1 && features[2] >= 0 && features[2] <= 1;
    const gap = Number(judgeInputs.logitGap.value);
    const pDirect = sigmoid(0.35 - 0.1 * features[0] - gap / 2);
    const pParallel = sigmoid(0.35 + 0.15 * features[1] + gap / 2);
    const absDiff = Math.abs(pParallel - pDirect);
    const samplesCount = Number(judgeInputs.samples.value) || 0;

    const gates = [
      { key: 'no_model', label: '1. Artifact present (O_NOFOLLOW <= 64 KiB)', ok: judgeInputs.modelPresent.checked },
      { key: 'invalid_model', label: '2. schema_version==1, job_limits_v1, |w|<=100', ok: judgeInputs.validSchema.checked },
      { key: 'domain_mismatch', label: '3. domain_sha256 exact match', ok: judgeInputs.domainMatch.checked },
      { key: 'no_verifier', label: '4. Fixed external verifier configured', ok: judgeInputs.verifier.checked },
      { key: 'outside_support', label: '5. Features inside [feature_min, feature_max]', ok: inSupport },
      { key: 'invalid_prediction', label: '6. Cost heads log1p_tok/sec in [0, 30]', ok: judgeInputs.costBounds.checked },
      { key: 'insufficient_samples', label: `7. training_samples >= 30 (current: ${samplesCount})`, ok: samplesCount >= 30 },
      { key: 'ambiguous', label: `8. Margin |Δp| >= 0.05 (current: ${absDiff.toFixed(3)})`, ok: absDiff >= 0.05 },
    ];

    const firstFail = gates.find((g) => !g.ok);
    if (firstFail) {
      judgeDecisionEl.className = 'status-banner status-banner-warn';
      judgeDecisionEl.innerHTML = `<code>Abstain { reason: "${firstFail.key}" }</code> — Shadow log retained; execution strategy remains UCB/explicit.`;
    } else {
      const pref = pParallel > pDirect ? 'Parallel' : 'Direct';
      judgeDecisionEl.className = 'status-banner status-banner-pass';
      judgeDecisionEl.innerHTML = `<code>Rank { preferred: "${pref}" }</code> (status: <code>uncalibrated</code>; Δp=${absDiff.toFixed(3)}) — Experimental log only; never overrides host router.`;
    }

    judgeProbsEl.textContent = `Independent heads (not softmax): P(Direct)=${pDirect.toFixed(3)} · P(Parallel)=${pParallel.toFixed(3)} · |Δ|=${absDiff.toFixed(3)}`;
    judgeFeatEl.textContent = `[task_bytes=${features[0].toFixed(3)}, max_turns=${features[1].toFixed(3)}, max_repairs=${features[2].toFixed(3)}, has_limits=1, ...]`;

    judgeGatesEl.innerHTML = gates
      .map(
        (g) => `
        <div class="gate-row">
          <span class="gate-row-label">${escapeHtml(g.label)}</span>
          <span class="pill ${g.ok ? 'pill-pass' : 'pill-warn'}">${g.ok ? 'OPEN' : g.key}</span>
        </div>`,
      )
      .join('');
  }

  Object.values(judgeInputs).forEach((el) => {
    el.addEventListener('input', renderJudge);
    el.addEventListener('change', renderJudge);
  });

  renderUcb();
  renderJudge();
}

/* ── 4. Field Trials Explorer (<FieldTrialsExplorer>) ─────────────────────── */

async function loadTrialsData() {
  try {
    const resp = await fetch('./data/field_runs.json', { cache: 'no-cache' });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
    return await resp.json();
  } catch {
    return EMBEDDED_FIELD_RUNS;
  }
}

function initTrialsExplorer(dataset) {
  const trials = Array.isArray(dataset.trials) ? dataset.trials : [];
  const searchEl = document.getElementById('trial-search');
  const campaignEl = document.getElementById('trial-campaign');
  const strategyEl = document.getElementById('trial-strategy');
  const statusEl = document.getElementById('trial-status');
  const tbodyEl = document.getElementById('trials-tbody');
  const emptyEl = document.getElementById('trials-empty');
  const tableEl = document.getElementById('trials-table');
  const countPillEl = document.getElementById('trials-count-pill');
  const detailEl = document.getElementById('trial-detail');
  let selectedId = trials[11]?.trial || trials[0]?.trial || null;

  function statusBadgeClass(status) {
    if (status === 'verified') return 'pill-pass';
    if (status === 'infrastructure_error') return 'pill-warn';
    return 'pill-info';
  }

  function renderDetail(t) {
    if (!t) {
      detailEl.hidden = true;
      return;
    }
    detailEl.hidden = false;
    detailEl.innerHTML = `
      <strong>${escapeHtml(t.trial)} (${escapeHtml(t.campaign)})</strong> — Started <code class="mono">${escapeHtml(t.started_at)}</code> ·
      Wall time <code class="mono">${t.approx_wall_seconds}s</code> · Strategy <code class="mono">${escapeHtml(t.strategy)}</code> ·
      Baseline failures <code class="mono">${t.baseline_failures}</code> ·
      Verifier checks <code class="mono">${t.verifier_checks ?? 'not reached'}</code> ·
      Reward recorded to UCB1: <code class="mono">${t.reward_recorded}</code>
      ${t.error ? `<div style="margin-top:0.35rem;">Error: <code class="mono">${escapeHtml(t.error)}</code>${t.addressed_by ? ` (addressed by commit <code class="mono">${escapeHtml(t.addressed_by)}</code>)` : ''}</div>` : ''}
    `;
  }

  function renderTable() {
    const q = searchEl.value.trim().toLowerCase();
    const camp = campaignEl.value;
    const strat = strategyEl.value;
    const st = statusEl.value;

    const filtered = trials.filter((t) => {
      if (camp !== 'all' && t.campaign !== camp) return false;
      if (strat !== 'all' && t.strategy !== strat) return false;
      if (st !== 'all' && t.status !== st) return false;
      if (q) {
        const hay = `${t.trial} ${t.campaign} ${t.strategy} ${t.status} ${t.failure_class || ''} ${t.error || ''} ${t.addressed_by || ''}`.toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });

    countPillEl.textContent = `${filtered.length} / ${trials.length} trials shown`;

    if (filtered.length === 0) {
      tableEl.hidden = true;
      emptyEl.hidden = false;
      detailEl.hidden = true;
      return;
    }

    tableEl.hidden = false;
    emptyEl.hidden = true;
    if (!filtered.some((t) => t.trial === selectedId)) {
      selectedId = filtered[0].trial;
    }

    tbodyEl.innerHTML = filtered
      .map(
        (t) => `
        <tr data-trial-id="${escapeHtml(t.trial)}" data-selected="${String(t.trial === selectedId)}" tabindex="0">
          <td class="mono"><strong>${escapeHtml(t.trial)}</strong></td>
          <td class="mono">${escapeHtml(t.campaign)}</td>
          <td class="mono">${escapeHtml(t.strategy)}</td>
          <td><span class="pill ${statusBadgeClass(t.status)}">${escapeHtml(t.status)}</span></td>
          <td class="mono">${t.approx_wall_seconds}s</td>
          <td class="mono">${t.verifier_checks ?? '—'}</td>
          <td class="mono">${escapeHtml(t.failure_class || '—')}</td>
          <td class="mono">${escapeHtml(t.addressed_by || '—')}</td>
          <td class="mono">${t.reward_recorded ? 'yes (verified)' : 'no (0 update)'}</td>
        </tr>`,
      )
      .join('');

    tbodyEl.querySelectorAll('tr').forEach((row) => {
      const selectRow = () => {
        selectedId = row.dataset.trialId;
        renderTable();
      };
      row.addEventListener('click', selectRow);
      row.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          selectRow();
        }
      });
    });

    renderDetail(filtered.find((t) => t.trial === selectedId));
  }

  document.getElementById('trials-clear-btn').addEventListener('click', () => {
    searchEl.value = '';
    campaignEl.value = 'all';
    strategyEl.value = 'all';
    statusEl.value = 'all';
    renderTable();
  });

  [searchEl, campaignEl, strategyEl, statusEl].forEach((el) => {
    el.addEventListener('input', renderTable);
    el.addEventListener('change', renderTable);
  });

  renderTable();
}

/* ── 5. Figure Plates Gallery (<FigurePlatesGallery>) ─────────────────────── */

function initPlatesGallery() {
  const tabsEl = document.getElementById('plate-tabs');
  const imgEl = document.getElementById('plate-image');
  const titleEl = document.getElementById('plate-title');
  const captionEl = document.getElementById('plate-caption');
  const linkEl = document.getElementById('plate-svg-link');

  function selectPlate(plateId) {
    const plate = FIGURE_PLATES.find((p) => p.id === plateId) || FIGURE_PLATES[0];
    imgEl.src = plate.src;
    imgEl.alt = `${plate.title} — ${plate.caption}`;
    titleEl.textContent = plate.title;
    captionEl.textContent = plate.caption;
    linkEl.href = plate.src;
    tabsEl.querySelectorAll('.tab-btn').forEach((btn) => {
      btn.setAttribute('aria-selected', String(btn.dataset.plateId === plate.id));
    });
  }

  tabsEl.innerHTML = FIGURE_PLATES.map(
    (p, idx) =>
      `<button type="button" role="tab" class="tab-btn" data-plate-id="${escapeHtml(p.id)}" aria-selected="${idx === 0 ? 'true' : 'false'}">${escapeHtml(p.label)}</button>`,
  ).join('');

  tabsEl.addEventListener('click', (e) => {
    const btn = e.target.closest('[data-plate-id]');
    if (btn) selectPlate(btn.dataset.plateId);
  });

  tabsEl.addEventListener('keydown', (e) => {
    const buttons = Array.from(tabsEl.querySelectorAll('.tab-btn'));
    const cur = buttons.indexOf(document.activeElement);
    if (cur === -1) return;
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      const next = buttons[(cur + 1) % buttons.length];
      next.focus();
      selectPlate(next.dataset.plateId);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      const prev = buttons[(cur - 1 + buttons.length) % buttons.length];
      prev.focus();
      selectPlate(prev.dataset.plateId);
    }
  });

  selectPlate('fig1');
}

function hydrateHeroMetadata() {
  const elCommit = document.getElementById('meta-commit');
  const elLoc = document.getElementById('meta-loc');
  const elTests = document.getElementById('meta-tests');
  if (elCommit) elCommit.textContent = `HEAD · ${REPORT_META.baselineCommit}`;
  if (elLoc) elLoc.textContent = `${REPORT_META.loc.toLocaleString()} LOC`;
  if (elTests) elTests.textContent = `${REPORT_META.tests} / ${REPORT_META.suites} suites`;
}

document.addEventListener('DOMContentLoaded', async () => {
  hydrateHeroMetadata();
  initSchedulerLab();
  initScopeLab();
  initRouterAndJudgeLab();
  initPlatesGallery();
  const dataset = await loadTrialsData();
  initTrialsExplorer(dataset);
});

