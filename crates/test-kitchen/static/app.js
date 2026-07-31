const state = {
  manifest: null,
  current: null,
  dirty: false,
  beats: [],
  tapTimes: [],
  tappedBpm: null,
};

const byId = (id) => document.getElementById(id);
const player = byId("audio-player");

async function request(url, options = {}) {
  const response = await fetch(url, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers || {}),
    },
  });
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const body = await response.json();
      message = body.error || message;
    } catch {
      // Keep the HTTP status when the response is not JSON.
    }
    throw new Error(message);
  }
  if (response.status === 204) return null;
  return response.json();
}

function setStatus(message, error = false) {
  const element = byId("save-status");
  element.textContent = message;
  element.classList.toggle("error", error);
}

function markDirty() {
  state.dirty = true;
  setStatus("Unsaved changes");
}

function formatTime(seconds) {
  if (!Number.isFinite(seconds)) return "0:00.000";
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds - minutes * 60;
  return `${minutes}:${remainder.toFixed(3).padStart(6, "0")}`;
}

function numberOrNull(value) {
  if (value === "") return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function textOrNull(value) {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

function currentPosition() {
  return Number.isFinite(player.currentTime) ? Number(player.currentTime.toFixed(3)) : 0;
}

function suggestedEnd(start) {
  if (Number.isFinite(player.duration) && player.duration > start) {
    return Number(player.duration.toFixed(3));
  }
  return Number((start + 1).toFixed(3));
}

function renderTrackList() {
  const list = byId("track-list");
  list.replaceChildren();
  for (const track of state.manifest.tracks) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "track-button";
    button.textContent = track.id;
    button.classList.toggle("active", state.current?.id === track.id);
    button.addEventListener("click", () => selectTrack(track.id));
    list.append(button);
  }
}

function selectTrack(id) {
  if (state.dirty && !window.confirm("Discard unsaved annotation changes?")) return;
  const track = state.manifest.tracks.find((candidate) => candidate.id === id);
  if (!track) return;
  state.current = structuredClone(track);
  state.beats = structuredClone(track.expected_beats || []);
  state.dirty = false;
  state.tapTimes = [];
  state.tappedBpm = null;
  byId("use-tap-tempo").disabled = true;
  byId("tap-result").textContent = "Tap at least twice";
  byId("empty-state").classList.add("hidden");
  byId("editor").classList.remove("hidden");
  byId("track-title").textContent = track.id;
  byId("track-path").textContent = track.path;
  player.src = `/audio/${encodeURIComponent(track.id)}`;
  player.load();

  byId("expected-bpm").value = track.expected_bpm ?? "";
  byId("expected-key").value = track.expected_key ?? "";
  byId("split").value = track.split ?? "";
  byId("serato-bpm").value = track.serato?.bpm ?? "";
  byId("serato-key").value = track.serato?.key ?? "";
  byId("serato-bpm-edited").checked = track.serato?.bpm_user_edited ?? false;
  byId("serato-key-edited").checked = track.serato?.key_user_edited ?? false;
  byId("annotation-status").value = track.annotation?.status ?? "draft";
  byId("reviewer").value = track.annotation?.reviewer ?? "";
  byId("annotation-confidence").value = track.annotation?.confidence ?? "";
  byId("notes").value = track.annotation?.notes ?? "";
  byId("analysis-result").textContent = "Not analyzed in this session.";
  byId("analysis-result").classList.add("muted");

  renderChangeEvents(track.change_events || []);
  renderTempoSegments(track.expected_tempo_segments || []);
  renderKeySegments(track.expected_key_segments || []);
  renderBeats();
  renderTrackList();
  setStatus("Annotation loaded");
}

function cloneTemplate(id) {
  return byId(id).content.firstElementChild.cloneNode(true);
}

function wireRemoveButton(row) {
  row.querySelector('[data-action="remove-row"]').addEventListener("click", () => {
    row.remove();
    markDirty();
  });
  row.querySelectorAll("input").forEach((input) => input.addEventListener("input", markDirty));
}

function addChangeEventRow(event) {
  const row = cloneTemplate("change-event-template");
  row.querySelector('[data-field="time"]').value = event.time_seconds ?? currentPosition();
  row.querySelector('[data-field="label"]').value = event.label ?? "";
  row.querySelector('[data-field="structure"]').checked = event.structure_changed ?? true;
  row.querySelector('[data-field="tempo"]').checked = event.tempo_changed ?? false;
  row.querySelector('[data-field="key"]').checked = event.key_changed ?? false;
  wireRemoveButton(row);
  byId("change-events").append(row);
}

function renderChangeEvents(events) {
  byId("change-events").replaceChildren();
  events.forEach(addChangeEventRow);
}

function addTempoSegmentRow(segment) {
  const start = segment.start_seconds ?? currentPosition();
  const row = cloneTemplate("tempo-segment-template");
  row.querySelector('[data-field="start"]').value = start;
  row.querySelector('[data-field="end"]').value = segment.end_seconds ?? suggestedEnd(start);
  row.querySelector('[data-field="bpm"]').value = segment.bpm ?? byId("expected-bpm").value;
  row.querySelector('[data-field="end-bpm"]').value = segment.end_bpm ?? "";
  wireRemoveButton(row);
  byId("tempo-segments").append(row);
}

function renderTempoSegments(segments) {
  byId("tempo-segments").replaceChildren();
  segments.forEach(addTempoSegmentRow);
}

function addKeySegmentRow(segment) {
  const start = segment.start_seconds ?? currentPosition();
  const row = cloneTemplate("key-segment-template");
  row.querySelector('[data-field="start"]').value = start;
  row.querySelector('[data-field="end"]').value = segment.end_seconds ?? suggestedEnd(start);
  row.querySelector('[data-field="key"]').value = segment.key ?? byId("expected-key").value;
  wireRemoveButton(row);
  byId("key-segments").append(row);
}

function renderKeySegments(segments) {
  byId("key-segments").replaceChildren();
  segments.forEach(addKeySegmentRow);
}

function renderBeats() {
  const list = byId("beat-list");
  list.replaceChildren();
  state.beats.forEach((beat, index) => {
    const chip = document.createElement("span");
    chip.className = "chip";
    const time = document.createElement("span");
    time.textContent = formatTime(beat.time_seconds);
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "danger";
    remove.textContent = "×";
    remove.setAttribute("aria-label", `Remove beat at ${formatTime(beat.time_seconds)}`);
    remove.addEventListener("click", () => {
      state.beats.splice(index, 1);
      renderBeats();
      markDirty();
    });
    chip.append(time, remove);
    list.append(chip);
  });
}

function collectChangeEvents() {
  return [...document.querySelectorAll("#change-events .change-row")]
    .map((row) => ({
      time_seconds: numberOrNull(row.querySelector('[data-field="time"]').value),
      structure_changed: row.querySelector('[data-field="structure"]').checked,
      tempo_changed: row.querySelector('[data-field="tempo"]').checked,
      key_changed: row.querySelector('[data-field="key"]').checked,
      label: textOrNull(row.querySelector('[data-field="label"]').value),
    }))
    .sort((a, b) => a.time_seconds - b.time_seconds);
}

function collectTempoSegments() {
  return [...document.querySelectorAll("#tempo-segments .segment-row")]
    .map((row) => ({
      start_seconds: numberOrNull(row.querySelector('[data-field="start"]').value),
      end_seconds: numberOrNull(row.querySelector('[data-field="end"]').value),
      bpm: numberOrNull(row.querySelector('[data-field="bpm"]').value),
      end_bpm: numberOrNull(row.querySelector('[data-field="end-bpm"]').value),
    }))
    .sort((a, b) => a.start_seconds - b.start_seconds);
}

function collectKeySegments() {
  return [...document.querySelectorAll("#key-segments .segment-row")]
    .map((row) => ({
      start_seconds: numberOrNull(row.querySelector('[data-field="start"]').value),
      end_seconds: numberOrNull(row.querySelector('[data-field="end"]').value),
      key: row.querySelector('[data-field="key"]').value.trim(),
    }))
    .sort((a, b) => a.start_seconds - b.start_seconds);
}

function collectTrack() {
  const originalSerato = state.current.serato || {};
  const seratoBpm = numberOrNull(byId("serato-bpm").value);
  const seratoKey = textOrNull(byId("serato-key").value);
  const hasSerato =
    seratoBpm !== null ||
    seratoKey !== null ||
    byId("serato-bpm-edited").checked ||
    byId("serato-key-edited").checked ||
    (originalSerato.beat_grid_seconds || []).length > 0;

  return {
    ...state.current,
    split: textOrNull(byId("split").value),
    expected_bpm: numberOrNull(byId("expected-bpm").value),
    expected_key: textOrNull(byId("expected-key").value),
    expected_beats: [...state.beats].sort((a, b) => a.time_seconds - b.time_seconds),
    expected_tempo_segments: collectTempoSegments(),
    expected_key_segments: collectKeySegments(),
    change_events: collectChangeEvents(),
    serato: hasSerato
      ? {
          ...originalSerato,
          bpm: seratoBpm,
          key: seratoKey,
          bpm_user_edited: byId("serato-bpm-edited").checked,
          key_user_edited: byId("serato-key-edited").checked,
        }
      : null,
    annotation: {
      status: byId("annotation-status").value,
      reviewer: textOrNull(byId("reviewer").value),
      confidence: numberOrNull(byId("annotation-confidence").value),
      notes: textOrNull(byId("notes").value),
    },
  };
}

async function saveTrack() {
  if (!state.current) return;
  setStatus("Saving");
  try {
    const track = collectTrack();
    const saved = await request(`/api/tracks/${encodeURIComponent(track.id)}`, {
      method: "PUT",
      body: JSON.stringify(track),
    });
    const index = state.manifest.tracks.findIndex((item) => item.id === saved.id);
    state.manifest.tracks[index] = saved;
    state.current = structuredClone(saved);
    state.dirty = false;
    setStatus("Saved");
    renderTrackList();
  } catch (error) {
    setStatus(error.message, true);
  }
}

function keyLabel(key) {
  if (!key) return "No result";
  const tonic = {
    CSharp: "C#",
    DSharp: "D#",
    FSharp: "F#",
    GSharp: "G#",
    ASharp: "A#",
  }[key.tonic] || key.tonic;
  return `${tonic} ${key.mode.toLowerCase()}`;
}

function formatBpmWithAlternate(beat) {
  const primary = beat.global_bpm?.toFixed(3) ?? "No result";
  if (!beat.multi_tempo || beat.alternate_bpm == null) return primary;
  const coverage = Math.round(beat.alternate_coverage * 100);
  return `${primary}★ → ${beat.alternate_bpm.toFixed(1)} (${coverage}%)`;
}

function formatKeyWithAlternate(keyAnalysis) {
  const primary = keyLabel(keyAnalysis.key);
  if (!keyAnalysis.multi_key || !keyAnalysis.alternate_key) return primary;
  const coverage = Math.round(keyAnalysis.alternate_coverage * 100);
  return `${primary}★ → ${keyLabel(keyAnalysis.alternate_key)} (${coverage}%)`;
}

function metric(label, value) {
  const wrapper = document.createElement("div");
  wrapper.className = "metric";
  const name = document.createElement("span");
  name.textContent = label;
  const result = document.createElement("strong");
  result.textContent = value;
  wrapper.append(name, result);
  return wrapper;
}

async function runAnalysis() {
  if (!state.current) return;
  const button = byId("run-analysis");
  button.disabled = true;
  button.textContent = "Analyzing";
  const result = byId("analysis-result");
  result.textContent = "Decoding and analyzing locally";
  try {
    const analysis = await request(
      `/api/tracks/${encodeURIComponent(state.current.id)}/analyze`,
      { method: "POST", body: "{}" },
    );
    result.replaceChildren(
      metric("Global BPM", formatBpmWithAlternate(analysis.beat)),
      metric("Tempo confidence", analysis.beat.confidence.toFixed(3)),
      metric("Global key", formatKeyWithAlternate(analysis.key)),
      metric("Key confidence", analysis.key.confidence.toFixed(3)),
      metric("Tempo segments", String(analysis.beat.tempo_segments.length)),
      metric("Key segments", String(analysis.key.segments.length)),
      metric("Detected beats", String(analysis.beat.beats.length)),
      metric("Downbeats", String(analysis.beat.beats.filter(b => b.position_in_bar === 1).length)),
      metric("Duration", formatTime(analysis.duration_seconds)),
      metric("Waveform columns", String(analysis.waveform.columns.length)),
    );
    result.classList.remove("muted");
  } catch (error) {
    result.textContent = error.message;
    result.classList.add("muted");
  } finally {
    button.disabled = false;
    button.textContent = "Analyze track";
  }
}

function tapTempo() {
  const now = performance.now();
  const previous = state.tapTimes.at(-1);
  if (previous && now - previous > 2500) state.tapTimes = [];
  state.tapTimes.push(now);
  state.tapTimes = state.tapTimes.slice(-9);
  if (state.tapTimes.length < 2) {
    byId("tap-result").textContent = "Keep tapping";
    return;
  }
  const intervals = state.tapTimes.slice(1).map((time, index) => time - state.tapTimes[index]);
  const sorted = [...intervals].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  const median =
    sorted.length % 2 === 0
      ? (sorted[middle - 1] + sorted[middle]) / 2
      : sorted[middle];
  state.tappedBpm = 60000 / median;
  byId("tap-result").textContent = `${state.tappedBpm.toFixed(2)} BPM`;
  byId("use-tap-tempo").disabled = false;
}

async function addTrack(event) {
  event.preventDefault();
  const id = byId("new-track-id").value.trim();
  const path = byId("new-track-path").value.trim();
  const track = {
    id,
    path,
    expected_beats: [],
    expected_tempo_segments: [],
    expected_key_segments: [],
    change_events: [],
    annotation: { status: "draft" },
  };
  try {
    const created = await request("/api/tracks", {
      method: "POST",
      body: JSON.stringify(track),
    });
    state.manifest.tracks.push(created);
    byId("add-track-form").reset();
    byId("add-track-form").classList.add("hidden");
    renderTrackList();
    selectTrack(created.id);
  } catch (error) {
    setStatus(error.message, true);
  }
}

async function deleteTrack() {
  if (!state.current || !window.confirm(`Delete ${state.current.id} from the manifest?`)) return;
  try {
    await request(`/api/tracks/${encodeURIComponent(state.current.id)}`, {
      method: "DELETE",
    });
    state.manifest.tracks = state.manifest.tracks.filter(
      (track) => track.id !== state.current.id,
    );
    state.current = null;
    state.dirty = false;
    player.removeAttribute("src");
    byId("editor").classList.add("hidden");
    byId("empty-state").classList.remove("hidden");
    renderTrackList();
    setStatus("Track removed");
  } catch (error) {
    setStatus(error.message, true);
  }
}

function wireEvents() {
  byId("show-add-track").addEventListener("click", () =>
    byId("add-track-form").classList.remove("hidden"),
  );
  byId("cancel-add-track").addEventListener("click", () =>
    byId("add-track-form").classList.add("hidden"),
  );
  byId("add-track-form").addEventListener("submit", addTrack);
  byId("save-track").addEventListener("click", saveTrack);
  byId("delete-track").addEventListener("click", deleteTrack);
  byId("run-analysis").addEventListener("click", runAnalysis);
  byId("tap-tempo").addEventListener("click", tapTempo);
  byId("use-tap-tempo").addEventListener("click", () => {
    byId("expected-bpm").value = state.tappedBpm.toFixed(3);
    markDirty();
  });
  byId("add-change-event").addEventListener("click", () => {
    addChangeEventRow({ time_seconds: currentPosition(), structure_changed: true });
    markDirty();
  });
  byId("add-tempo-segment").addEventListener("click", () => {
    addTempoSegmentRow({});
    markDirty();
  });
  byId("add-key-segment").addEventListener("click", () => {
    addKeySegmentRow({});
    markDirty();
  });
  byId("add-beat").addEventListener("click", () => {
    state.beats.push({ time_seconds: currentPosition() });
    state.beats.sort((a, b) => a.time_seconds - b.time_seconds);
    renderBeats();
    markDirty();
  });
  player.addEventListener("timeupdate", () => {
    byId("current-time").textContent = formatTime(player.currentTime);
  });
  document
    .querySelectorAll("#editor input, #editor select, #editor textarea")
    .forEach((input) => input.addEventListener("input", markDirty));
  window.addEventListener("beforeunload", (event) => {
    if (state.dirty) event.preventDefault();
  });
}

async function initialize() {
  wireEvents();
  try {
    state.manifest = await request("/api/manifest");
    renderTrackList();
    setStatus(`${state.manifest.tracks.length} tracks loaded`);
    if (state.manifest.tracks.length > 0) selectTrack(state.manifest.tracks[0].id);
  } catch (error) {
    setStatus(error.message, true);
  }
}

initialize();
