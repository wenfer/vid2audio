const api = "/api/v1";

let collections = [];
let jobs = [];
let selected = null;
let selectedJob = null;
let settings = null;
let polling = null;
let accelerationInfo = null;
let browserState = null;
let selectedPath = "";
let activeView = "workspace";

const $ = (id) => document.getElementById(id);

async function request(path, options = {}) {
  const response = await fetch(`${api}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!response.ok) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.detail || response.statusText);
  }
  return response.json();
}

function setStatus(message, kind = "") {
  $("status").className = `status ${kind}`;
  $("status").textContent = message;
}

async function loadSettings() {
  settings = await request("/settings");
  $("sourcePath").value = settings.scan_directories[0] || "";
  selectedPath = $("sourcePath").value;
  $("defaultSourcePath").value = settings.scan_directories[0] || "";
  $("outputDir").value = settings.output_directory;
  $("minFileSize").value = settings.min_file_size_mb;
  $("videoExtensions").value = settings.video_extensions.join(", ");
  $("ignoredExtensions").value = settings.ignored_extensions.join(", ");
  $("filesystemSorting").value = settings.filesystem_sorting || "ntfs";
  $("paddingDigits").value = settings.padding_digits || "auto";
  $("defaultFormat").value = settings.default_output_format;
  $("defaultQuality").value = settings.default_quality;
  $("ttsProvider").value = settings.tts_provider || (settings.tts_enabled ? "edge" : "disabled");
  $("ttsFailureMode").value = settings.tts_failure_mode || "silent";
  $("ttsVoice").value = settings.tts_voice || "zh-CN-XiaoxiaoNeural";
  $("ttsRate").value = settings.tts_rate || "+0%";
  $("introTextTemplate").value = settings.intro_text_template || "{collection_name}";
  $("hardwareAcceleration").value = settings.hardware_acceleration || "auto";
  $("hardwareAccelerationDevice").value = settings.hardware_acceleration_device || "";
  $("format").value = settings.default_output_format;
  $("quality").value = settings.default_quality;
  $("sampleRate").value = String(settings.default_sample_rate);
}

async function loadAccelerationInfo() {
  accelerationInfo = await request("/system/hardware-acceleration");
  renderAccelerationHint();
}

async function loadCollections() {
  collections = await request("/collections");
  renderCollections();
}

async function loadJobs() {
  jobs = await request("/extract/jobs");
  renderJobs();
}

async function loadFileBrowser(path = null) {
  const target = path || selectedPath || (settings && settings.scan_directories[0]) || "";
  const query = target ? `?path=${encodeURIComponent(target)}` : "";
  browserState = await request(`/files${query}`);
  if (browserState.warning) {
    selectedPath = browserState.path;
    $("sourcePath").value = browserState.path;
    $("jobName").value = browserState.path.split("/").filter(Boolean).pop() || "";
    $("title").textContent = "已选择";
    $("meta").textContent = browserState.path;
    $("taskSummary").textContent = "待分析";
  }
  $("browserPath").value = browserState.path;
  renderFileBrowser();
  if (browserState.warning) {
    setStatus(browserState.warning, "warning");
  }
}

function renderFileBrowser() {
  $("parentDirBtn").disabled = !browserState.parent;
  $("browserSummary").textContent = `${browserState.entries.length} 项 · ${sortLabel(browserState.sorting || "ntfs")}`;
  const rows = browserState.entries
    .map((entry) => {
      const icon = entry.type === "directory" ? "▸" : entry.is_video ? "♪" : "·";
      const selectable = entry.selectable ? "" : "disabled";
      const typeLabel = entry.type === "directory" ? "文件夹" : entry.is_video ? "视频" : "文件";
      const title = entry.type === "directory" ? "单击选择，双击进入文件夹" : "单击选择";
      return `
        <div class="browser-row ${selectedPath === entry.path ? "active" : ""} ${entry.selectable ? "" : "muted-row"}" data-path="${escapeAttr(entry.path)}" data-type="${entry.type}" data-selectable="${entry.selectable}" tabindex="${entry.selectable ? "0" : "-1"}" title="${escapeAttr(title)}">
          <button class="icon-button open-entry" title="${entry.type === "directory" ? "打开文件夹" : "选择文件"}">${icon}</button>
          <button class="text-button select-entry" tabindex="-1" ${selectable}>
            <span class="truncate">${escapeHtml(entry.name)}</span>
            <small>${typeLabel}${entry.reason ? " · " + escapeHtml(entry.reason) : ""}</small>
          </button>
          <div>${entry.type === "file" ? formatBytes(entry.size) : ""}</div>
        </div>
      `;
    })
    .join("");
  $("fileBrowser").innerHTML = rows || '<div class="empty-state">这个目录是空的</div>';
  document.querySelectorAll(".browser-row").forEach((row) => {
    row.addEventListener("click", () => handleBrowserRowClick(row));
    row.addEventListener("dblclick", () => handleBrowserRowOpen(row));
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        handleBrowserRowOpen(row);
      } else if (event.key === " ") {
        event.preventDefault();
        handleBrowserRowClick(row);
      }
    });
    row.querySelector(".open-entry").addEventListener("click", (event) => {
      event.stopPropagation();
      handleBrowserRowOpen(row);
    });
  });
}

function handleBrowserRowClick(row) {
  if (row.dataset.selectable !== "true") return;
  choosePath(row.dataset.path);
}

function handleBrowserRowOpen(row) {
  if (row.dataset.selectable !== "true") return;
  const path = row.dataset.path;
  if (row.dataset.type === "directory") {
    loadFileBrowser(path).catch((error) => setStatus(error.message, "error"));
  } else {
    choosePath(path);
  }
}

function choosePath(path) {
  selectedPath = path;
  $("sourcePath").value = path;
  $("jobName").value = path.split("/").filter(Boolean).pop() || "";
  $("title").textContent = "已选择";
  $("meta").textContent = path;
  $("taskSummary").textContent = "待分析";
  renderFileBrowser();
  setStatus("已选择: " + path);
}

function renderCollections() {
  $("collections").innerHTML = collections
    .map(
      (item) => `
        <button class="collection ${selected && selected.id === item.id ? "active" : ""}" data-id="${item.id}">
          ${escapeHtml(item.name)}
          <span>${item.episode_count} 个视频 · ${escapeHtml(item.status)}</span>
        </button>
      `
    )
    .join("");
  document.querySelectorAll("#collections .collection").forEach((button) => {
    button.addEventListener("click", () => selectCollection(button.dataset.id));
  });
}

function renderJobs() {
  $("jobSummary").textContent = `${jobs.length} 个任务`;
  $("jobs").innerHTML = jobs
    .map(
      (job) => `
        <button class="collection ${selectedJob && selectedJob.id === job.id ? "active" : ""}" data-id="${job.id}">
          ${escapeHtml(job.name || job.id.slice(0, 8))}
          <span>${escapeHtml(job.status)} · ${job.progress}% · 成功 ${job.success_count} / 失败 ${job.failure_count}</span>
        </button>
      `
    )
    .join("");
  document.querySelectorAll("#jobs .collection").forEach((button) => {
    button.addEventListener("click", () => selectJob(button.dataset.id));
  });
}

async function selectCollection(id) {
  showView("workspace");
  selectedJob = null;
  selected = await request(`/collections/${id}`);
  renderCollections();
  renderJobs();
  renderCollectionDetail();
  syncTaskControlsFromSelection();
}

async function selectJob(id) {
  showView("tasks");
  selected = null;
  selectedJob = await request(`/extract/jobs/${id}`);
  renderCollections();
  renderJobs();
  renderJobDetail();
}

function renderCollectionDetail() {
  $("title").textContent = selected.name;
  $("meta").textContent = `${selected.source_path} · ${selected.episode_count} 个视频`;
  const rows = selected.video_files
    .map((video, index) => {
      return `
        <div class="file-row">
          <div>${String(index + 1).padStart(3, "0")}</div>
          <div class="truncate" title="${escapeAttr(video.filename)}">${escapeHtml(video.episode_title)}</div>
          <div>${escapeHtml(trackSummary(video.audio_tracks))}</div>
          <div>${formatDuration(video.duration)}</div>
        </div>
      `;
    })
    .join("");
  $("detail").className = "detail";
  $("detail").innerHTML = `
    <div class="file-list">
      <div class="file-row header">
        <div>序号</div><div>标题</div><div>可用音轨</div><div>时长</div>
      </div>
      ${rows || '<div class="empty-state">没有符合过滤条件的视频文件</div>'}
    </div>
    <pre class="output-preview">${escapeHtml(outputPreview())}</pre>
  `;
}

function renderJobDetail() {
  $("title").textContent = selectedJob.name || "音频提取任务";
  $("meta").textContent = `${selectedJob.source_path || "未知源路径"} · ${selectedJob.status}`;
  const rows = selectedJob.items
    .map(
      (item) => `
        <div class="job-row">
          <div class="${item.status === "failed" ? "error" : ""}">${escapeHtml(item.status)}</div>
          <div class="truncate" title="${escapeAttr(item.source_path)}">${escapeHtml(item.title || item.source_path)}</div>
          <div class="output-cell">
            <span class="truncate" title="${escapeAttr(item.output_path || item.error_message || "")}">${escapeHtml(item.output_path || item.error_message || "")}</span>
            ${item.status === "completed" && item.output_path ? `<button class="small-button play-output" data-item-id="${escapeAttr(item.id)}">播放</button>` : ""}
          </div>
        </div>
      `
    )
    .join("");
  $("detail").className = "detail";
  $("detail").innerHTML = `
    <div class="summary">
      <div><strong>${selectedJob.progress}%</strong><span>进度</span></div>
      <div><strong>${selectedJob.total_count}</strong><span>总数</span></div>
      <div><strong>${selectedJob.success_count}</strong><span>成功</span></div>
      <div><strong>${selectedJob.failure_count}</strong><span>失败</span></div>
    </div>
    <div class="progress"><div style="width:${selectedJob.progress}%"></div></div>
    <p>${escapeHtml(selectedJob.current_file || selectedJob.error_message || "")}</p>
    <div class="file-list job-list">
      <div class="job-row header"><div>状态</div><div>文件</div><div>输出或失败原因</div></div>
      ${rows || '<div class="empty-state">暂无任务明细</div>'}
    </div>
    <pre class="output-preview">${escapeHtml(JSON.stringify(selectedJob.summary || {}, null, 2))}</pre>
  `;
  document.querySelectorAll(".play-output").forEach((button) => {
    button.addEventListener("click", () => playOutputAudio(button.dataset.itemId));
  });
}

function syncTaskControlsFromSelection() {
  const tracks = collectTracks(selected);
  $("trackIndex").innerHTML = tracks
    .map((track) => `<option value="${track.index}">${escapeHtml(track.label)}</option>`)
    .join("");
  $("jobName").value = selected.name;
  $("taskSummary").textContent = `${selected.episode_count} 个视频`;
  renderAccelerationHint();
}

function renderAccelerationHint() {
  const supported = accelerationInfo && accelerationInfo.supported ? accelerationInfo.supported : [];
  const note = accelerationInfo ? accelerationInfo.note : "";
  const recommended = accelerationInfo && accelerationInfo.recommended ? accelerationInfo.recommended : "safe";
  $("accelerationHint").textContent =
    `硬件加速: ${supported.length ? supported.join(", ") : "未检测到可用后端"}。自动选择: ${recommended}。${note || "自动模式不可用时会使用 CPU。"}`;
}

function collectTracks(collection) {
  const first = collection && collection.video_files.find((video) => video.audio_tracks.length);
  if (!first) return [{ index: 0, label: "默认音轨" }];
  return first.audio_tracks.map((track) => ({
    index: track.index,
    label: `${track.language_full || "未知语言"} · ${track.codec || "audio"} · ${track.channels || "?"}ch · stream ${track.index}`,
  }));
}

function trackSummary(tracks) {
  if (!tracks.length) return "未解析";
  return tracks.map((track) => `${track.language_full || "未知"} ${track.codec || ""} #${track.index}`).join(" / ");
}

function outputPreview() {
  const ext = $("format").value || settings.default_output_format;
  const lines = [`${settings.output_directory}/${selected.name}/`];
  if (($("ttsProvider").value || "edge") !== "disabled") {
    lines.push(`├── 000_${selected.name}.${ext}`);
  }
  const padding = previewPadding(selected.video_files.length);
  selected.video_files.forEach((video, index) => {
    const branch = index === selected.video_files.length - 1 ? "└──" : "├──";
    lines.push(`${branch} ${String(index + 1).padStart(padding, "0")}_${video.episode_title}.${ext}`);
  });
  return lines.join("\n");
}

async function detectSource() {
  try {
    setStatus("正在分析选中项...");
    const sourcePath = $("sourcePath").value.trim();
    const result = await request("/scan/start", {
      method: "POST",
      body: JSON.stringify({ source_paths: [sourcePath] }),
    });
    setStatus(
      `分析完成: ${result.files_found} 个视频${result.warnings.length ? "，部分文件已过滤或解析失败" : ""}`,
      result.warnings.length ? "warning" : ""
    );
    await loadCollections();
    if (result.collections.length) {
      await selectCollection(result.collections[0].id);
    }
  } catch (error) {
    setStatus(error.message, "error");
  }
}

async function previewSelected() {
  try {
    if (!selected || !selected.video_files.length) throw new Error("请先分析选中项并选择合集");
    const video = selected.video_files[0];
    const track = Number($("trackIndex").value);
    const start = Number($("trimStart").value || 0);
    const audio = $("previewAudio");
    audio.src = `${api}/preview/${video.id}?track=${track}&duration=10&start=${start}&_=${Date.now()}`;
    audio.classList.remove("hidden");
    await audio.play().catch(() => {});
    setStatus(`正在试听: ${video.filename}`);
  } catch (error) {
    setStatus(error.message, "error");
  }
}

async function playOutputAudio(itemId) {
  try {
    if (!selectedJob) throw new Error("请先选择一个任务");
    if (!itemId) throw new Error("任务文件无效");
    const item = selectedJob.items.find((entry) => entry.id === itemId);
    const audio = $("previewAudio");
    audio.src = `${api}/extract/jobs/${selectedJob.id}/items/${itemId}/audio?_=${Date.now()}`;
    audio.classList.remove("hidden");
    await audio.play().catch(() => {});
    setStatus(`正在播放: ${item ? item.title || item.output_path : itemId}`);
  } catch (error) {
    setStatus(error.message, "error");
  }
}

async function extractSelected() {
  try {
    if (!selected) throw new Error("请先分析选中项并选择合集");
    setStatus("任务已创建，后台开始提取...");
    const job = await request("/extract", {
      method: "POST",
      body: JSON.stringify({
        collection_id: selected.id,
        job_name: $("jobName").value.trim() || selected.name,
        track_index: Number($("trackIndex").value),
        output_format: $("format").value,
        quality: $("quality").value,
        sample_rate: Number($("sampleRate").value),
        trim_start_seconds: Number($("trimStart").value || 0),
        trim_end_seconds: Number($("trimEnd").value || 0),
        hardware_acceleration: $("jobHardwareAcceleration").value || null,
        generate_intro: $("ttsProvider").value !== "disabled",
        tts_provider: $("ttsProvider").value,
        tts_rate: $("ttsRate").value.trim() || "+0%",
        tts_failure_mode: $("ttsFailureMode").value,
        filesystem_sorting: $("filesystemSorting").value,
        padding_digits: $("paddingDigits").value,
      }),
    });
    await loadJobs();
    await selectJob(job.id);
    startPolling(job.id);
  } catch (error) {
    setStatus(error.message, "error");
  }
}

function startPolling(jobId) {
  if (polling) clearInterval(polling);
  polling = setInterval(async () => {
    const job = await request(`/extract/jobs/${jobId}`);
    selectedJob = job;
    await loadJobs();
    renderJobDetail();
    if (["completed", "failed", "cancelled"].includes(job.status)) {
      clearInterval(polling);
      polling = null;
      setStatus(`任务结束: 成功 ${job.success_count}，失败 ${job.failure_count}`, job.failure_count ? "warning" : "");
    }
  }, 1200);
}

async function saveSettings() {
  try {
    settings = await request("/settings", {
      method: "PUT",
      body: JSON.stringify({
        scan_directories: [$("defaultSourcePath").value.trim()].filter(Boolean),
        output_directory: $("outputDir").value,
        min_file_size_mb: Number($("minFileSize").value || 0),
        video_extensions: csv($("videoExtensions").value),
        ignored_extensions: csv($("ignoredExtensions").value),
        filesystem_sorting: $("filesystemSorting").value,
        padding_digits: $("paddingDigits").value,
        default_output_format: $("defaultFormat").value,
        default_quality: $("defaultQuality").value,
        tts_enabled: $("ttsProvider").value !== "disabled",
        tts_provider: $("ttsProvider").value,
        tts_failure_mode: $("ttsFailureMode").value,
        tts_voice: $("ttsVoice").value.trim() || "zh-CN-XiaoxiaoNeural",
        tts_rate: $("ttsRate").value.trim() || "+0%",
        intro_text_template: $("introTextTemplate").value.trim() || "{collection_name}",
        hardware_acceleration: $("hardwareAcceleration").value,
        hardware_acceleration_device: $("hardwareAccelerationDevice").value.trim(),
        hardware_acceleration_fallback: true,
      }),
    });
    setStatus("全局配置已保存");
    if (selected) renderCollectionDetail();
  } catch (error) {
    setStatus(error.message, "error");
  }
}

function csv(value) {
  return value.split(",").map((item) => item.trim()).filter(Boolean);
}

function formatDuration(value) {
  if (!value) return "-";
  const minutes = Math.floor(value / 60);
  const seconds = Math.floor(value % 60);
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function formatBytes(value) {
  if (!value) return "";
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`;
  return `${(value / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function previewPadding(total) {
  const configured = $("paddingDigits").value || (settings && settings.padding_digits) || "auto";
  if (configured !== "auto") return Math.max(Number(configured) || 3, 1);
  return total < 1000 ? 3 : 4;
}

function sortLabel(value) {
  return {
    ntfs: "NTFS/FAT 兼容排序",
    natural: "自然数字排序",
    name: "按名称排序",
  }[value] || "NTFS/FAT 兼容排序";
}

function showView(view) {
  activeView = view;
  document.querySelectorAll(".menu-item").forEach((button) => {
    button.classList.toggle("active", button.dataset.view === view);
  });
  $("browserPanel").classList.toggle("hidden", view !== "workspace");
  $("taskPanel").classList.toggle("hidden", view !== "workspace");
  $("jobPanel").classList.toggle("hidden", view !== "tasks");
  $("settingsPanel").classList.toggle("hidden", view !== "settings");
  $("detail").classList.toggle("hidden", view === "settings");
  if (view === "tasks" && !selectedJob) {
    $("title").textContent = "任务管理";
    $("meta").textContent = "查看提取进度、任务简报和导出音频";
    $("detail").className = "detail empty";
    $("detail").innerHTML = '<div class="empty-state">请选择一个任务</div>';
  }
  if (view === "settings") {
    $("title").textContent = "系统配置";
    $("meta").textContent = "管理扫描过滤、排序、TTS 和硬件加速";
  }
}

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, (char) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#039;",
  })[char]);
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/"/g, "&quot;");
}

$("scanBtn").addEventListener("click", detectSource);
$("refreshFilesBtn").addEventListener("click", () => loadFileBrowser($("browserPath").value).catch((error) => setStatus(error.message, "error")));
$("openPathBtn").addEventListener("click", () => loadFileBrowser($("browserPath").value).catch((error) => setStatus(error.message, "error")));
$("parentDirBtn").addEventListener("click", () => browserState && browserState.parent && loadFileBrowser(browserState.parent).catch((error) => setStatus(error.message, "error")));
$("settingsBtn").addEventListener("click", () => showView("settings"));
document.querySelectorAll(".menu-item").forEach((button) => {
  button.addEventListener("click", () => showView(button.dataset.view));
});
$("saveSettingsBtn").addEventListener("click", saveSettings);
$("previewBtn").addEventListener("click", previewSelected);
$("extractBtn").addEventListener("click", extractSelected);

loadSettings()
  .then(loadAccelerationInfo)
  .then(() => loadFileBrowser())
  .then(loadCollections)
  .then(loadJobs)
  .then(() => showView(activeView))
  .catch((error) => setStatus(error.message, "error"));
