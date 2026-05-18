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
let confirmHandler = null;

const $ = (id) => document.getElementById(id);

// ===== API Helper =====
async function request(path, options = {}) {
  const response = await fetch(`${api}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!response.ok) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.detail || response.statusText);
  }
  if (response.status === 204) return null;
  const contentType = response.headers.get("content-type") || "";
  return contentType.includes("application/json") ? response.json() : response.text();
}

// ===== Toast =====
function toast(message, kind = "info") {
  const node = document.createElement("div");
  node.className = `toast ${kind}`;
  node.textContent = message;
  $("toastStack").appendChild(node);
  requestAnimationFrame(() => node.classList.add("show"));
  setTimeout(() => {
    node.classList.remove("show");
    setTimeout(() => node.remove(), 250);
  }, 3000);
}

// ===== Loading =====
function showLoading(title, message) {
  $("loadingTitle").textContent = title;
  $("loadingMessage").textContent = message;
  $("loadingOverlay").classList.remove("hidden");
}

function hideLoading() {
  $("loadingOverlay").classList.add("hidden");
}

// ===== Modal =====
function openModal(id) {
  $(id).classList.remove("hidden");
  $(id).setAttribute("aria-hidden", "false");
}

function closeModal(id) {
  $(id).classList.add("hidden");
  $(id).setAttribute("aria-hidden", "true");
}

function confirmDialog(title, message, actionLabel, action) {
  $("confirmTitle").textContent = title;
  $("confirmMessage").textContent = message;
  $("confirmActionBtn").textContent = actionLabel;
  confirmHandler = action;
  openModal("confirmModal");
}

// ===== View Switching =====
function showView(view) {
  activeView = view;
  document.querySelectorAll(".nav-tab").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.view === view);
  });
  $("workspaceView").classList.toggle("hidden", view !== "workspace");
  $("tasksView").classList.toggle("hidden", view !== "tasks");
  $("settingsView").classList.toggle("hidden", view !== "settings");
}

// ===== Settings =====
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
  $("ttsProvider").value = settings.tts_provider || (settings.tts_enabled ? "piper" : "disabled");
  $("ttsFailureMode").value = settings.tts_failure_mode || "silent";
  $("ttsVoice").value = settings.tts_voice || "zh_CN-huayan-medium";
  $("ttsRate").value = settings.tts_rate || "+0%";
  $("introTextTemplate").value = settings.intro_text_template || "{collection_name}";
  $("hardwareAcceleration").value = settings.hardware_acceleration || "auto";
  $("hardwareAccelerationDevice").value = settings.hardware_acceleration_device || "";
  $("hardwareAccelerationFallback").value = settings.hardware_acceleration_fallback !== false ? "true" : "false";
  $("format").value = settings.default_output_format;
  $("quality").value = settings.default_quality;
  $("sampleRate").value = String(settings.default_sample_rate);
}

async function saveSettings() {
  try {
    showLoading("正在保存配置", "正在写入全局设置…");
    settings = await request("/settings", {
      method: "PUT",
      body: JSON.stringify({
        scan_directories: [$("defaultSourcePath").value.trim()].filter(Boolean),
        output_directory: $("outputDir").value.trim(),
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
        tts_voice: $("ttsVoice").value.trim() || "zh_CN-huayan-medium",
        tts_rate: $("ttsRate").value.trim() || "+0%",
        intro_text_template: $("introTextTemplate").value.trim() || "{collection_name}",
        hardware_acceleration: $("hardwareAcceleration").value,
        hardware_acceleration_device: $("hardwareAccelerationDevice").value.trim(),
        hardware_acceleration_fallback: $("hardwareAccelerationFallback").value === "true",
      }),
    });
    await loadSettings();
    await loadFileBrowser($("browserPath").value || settings.scan_directories[0] || "");
    toast("全局配置已保存", "success");
    if (selected) renderCollectionDetail();
  } catch (error) {
    toast(error.message, "error");
  } finally {
    hideLoading();
  }
}

// ===== Hardware Acceleration =====
async function loadAccelerationInfo() {
  accelerationInfo = await request("/system/hardware-acceleration");
  renderAccelerationPanel();
  renderAccelerationHint();
}

async function redetectAcceleration() {
  try {
    const btn = $("redetectBtn");
    btn.disabled = true;
    btn.textContent = "检测中…";
    accelerationInfo = await request("/system/hardware-acceleration/detect", { method: "POST" });
    renderAccelerationPanel();
    renderAccelerationHint();
    toast("硬件加速检测完成", "success");
  } catch (error) {
    toast(error.message, "error");
  } finally {
    const btn = $("redetectBtn");
    btn.disabled = false;
    btn.innerHTML = `<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg> 重新检测`;
  }
}

function renderAccelerationPanel() {
  if (!accelerationInfo) return;

  // Status indicator
  const dot = document.querySelector(".status-dot");
  const statusText = $("hwaccelStatusText");
  if (accelerationInfo.available) {
    dot.className = "status-dot available";
    const count = accelerationInfo.supported ? accelerationInfo.supported.length : 0;
    statusText.textContent = `已检测到 ${count} 个加速后端`;
  } else {
    dot.className = "status-dot unavailable";
    statusText.textContent = "未检测到硬件加速";
  }

  // FFmpeg version
  const versionEl = $("hwaccelFfmpegVersion");
  versionEl.textContent = accelerationInfo.ffmpeg_version ? `FFmpeg ${accelerationInfo.ffmpeg_version}` : "FFmpeg 未安装";

  // Backend cards
  const backends = accelerationInfo.backends || [];
  const backendIcons = { cpu: "🖥️", chip: "🔲", gpu: "🎮", arm: "📱", apple: "🍎" };

  $("hwaccelBackends").innerHTML = backends.map((b) => {
    const classes = [
      "backend-card",
      b.detected ? "detected" : "not-detected",
      b.is_recommended ? "recommended" : "",
    ].filter(Boolean).join(" ");

    let badgeHtml = "";
    if (b.is_recommended) {
      badgeHtml = `<span class="backend-badge recommended-badge">推荐</span>`;
    } else if (b.detected) {
      badgeHtml = `<span class="backend-badge detected-badge">可用</span>`;
    } else {
      badgeHtml = `<span class="backend-badge not-detected-badge">未检测到</span>`;
    }

    return `
      <div class="${classes}">
        <div class="backend-icon">${backendIcons[b.icon] || "⚡"}</div>
        <div class="backend-info">
          <div class="backend-name">${escapeHtml(b.name)}</div>
          <div class="backend-desc">${escapeHtml(b.description)}</div>
        </div>
        ${badgeHtml}
      </div>
    `;
  }).join("");

  // Recommendation
  const rec = $("hwaccelRecommendation");
  const recommended = backends.find((b) => b.is_recommended);
  if (recommended) {
    rec.innerHTML = `
      <div class="rec-title">💡 系统建议</div>
      <div>${escapeHtml(accelerationInfo.note || "")}${recommended.note ? " " + escapeHtml(recommended.note) : ""}</div>
    `;
  } else {
    rec.innerHTML = `<div class="rec-title">💡 提示</div><div>${escapeHtml(accelerationInfo.note || "音频提取主要处理音频流，CPU 模式通常已足够快。")}</div>`;
  }

  // Update device hint based on recommended backend
  if (recommended && recommended.device_hint) {
    $("hardwareAccelerationDevice").placeholder = recommended.device_hint + "（可留空自动检测）";
  }

  // Config hint
  const currentStrategy = $("hardwareAcceleration").value;
  const strategyBackend = backends.find((b) => b.id === currentStrategy);
  if (currentStrategy === "auto") {
    $("hwaccelConfigHint").textContent = `自动模式将使用「${recommended ? recommended.name : "CPU"}」。失败时${$("hardwareAccelerationFallback").value === "true" ? "自动回退 CPU" : "任务将失败"}。`;
  } else if (strategyBackend && !strategyBackend.detected && currentStrategy !== "safe") {
    $("hwaccelConfigHint").textContent = `⚠️ 当前选择的「${strategyBackend.name}」未在系统中检测到，可能导致提取失败。建议切换为自动模式。`;
    $("hwaccelConfigHint").style.color = "var(--warning-text)";
  } else {
    $("hwaccelConfigHint").textContent = `当前策略: ${strategyBackend ? strategyBackend.name : currentStrategy}。${$("hardwareAccelerationFallback").value === "true" ? "失败时自动回退 CPU。" : ""}`;
    $("hwaccelConfigHint").style.color = "";
  }
}

function renderAccelerationHint() {
  const supported = accelerationInfo && accelerationInfo.supported ? accelerationInfo.supported : [];
  const recommended = accelerationInfo && accelerationInfo.recommended ? accelerationInfo.recommended : "safe";
  const backends = accelerationInfo && accelerationInfo.backends ? accelerationInfo.backends : [];
  const recBackend = backends.find((b) => b.is_recommended);
  const recName = recBackend ? recBackend.name : "CPU";
  $("accelerationHint").textContent = `当前加速: ${recName}${supported.length ? ` (${supported.length} 个后端可用)` : ""}。不可用时自动回退 CPU。`;
}

function renderJobAccelerationInfo(summary) {
  if (!summary || !summary.hardware_acceleration) return "";
  const accel = summary.hardware_acceleration;
  const fallbacks = accel.fallback_events || [];

  let html = `
    <div class="job-accel-info">
      <div class="accel-header">
        <svg viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z" clip-rule="evenodd"/></svg>
        硬件加速信息
      </div>
      <div class="accel-detail-row">
        <span class="accel-label">请求策略</span>
        <span class="accel-value">${escapeHtml(accel.requested || "auto")}</span>
      </div>
      <div class="accel-detail-row">
        <span class="accel-label">实际使用</span>
        <span class="accel-value">${escapeHtml(accel.resolved || "safe")}</span>
      </div>
      <div class="accel-detail-row">
        <span class="accel-label">回退事件</span>
        <span class="accel-value">${fallbacks.length ? fallbacks.length + " 次" : "无"}</span>
      </div>
  `;

  if (fallbacks.length > 0) {
    html += fallbacks.map((event) => `
      <div class="fallback-event">
        <div class="fallback-title">⚠️ 回退: ${escapeHtml(event.mode || "")} → CPU</div>
        <div>${escapeHtml(event.message || event.reason || "")}</div>
      </div>
    `).join("");
  }

  html += `</div>`;
  return html;
}

// ===== File Browser =====
async function loadFileBrowser(path = null) {
  const target = path || selectedPath || (settings && settings.scan_directories[0]) || "";
  const query = target ? `?path=${encodeURIComponent(target)}` : "";
  browserState = await request(`/files${query}`);
  if (browserState.warning) {
    selectedPath = browserState.path;
    $("sourcePath").value = browserState.path;
    $("jobName").value = browserState.path.split("/").filter(Boolean).pop() || "";
  }
  $("browserPath").value = browserState.path;
  renderFileBrowser();
  if (browserState.warning) {
    toast(browserState.warning, "warning");
  }
}

function renderFileBrowser() {
  $("parentDirBtn").disabled = !browserState.parent;
  $("browserSummary").textContent = `${browserState.entries.length} 项`;

  const rows = browserState.entries
    .map((entry) => {
      const iconClass = entry.type === "directory" ? "folder" : entry.is_video ? "video" : "other";
      const icon = entry.type === "directory" ? "📁" : entry.is_video ? "🎬" : "📄";
      const meta = entry.type === "directory" ? "文件夹" : entry.is_video ? "视频" : (entry.reason || "文件");
      return `
        <div class="browser-row ${selectedPath === entry.path ? "active" : ""} ${entry.selectable ? "" : "muted-row"}"
             data-path="${escapeAttr(entry.path)}" data-type="${entry.type}" data-selectable="${entry.selectable}"
             tabindex="${entry.selectable ? "0" : "-1"}">
          <div class="row-icon ${iconClass}">${icon}</div>
          <div class="row-info">
            <div class="row-name">${escapeHtml(entry.name)}</div>
            <div class="row-meta">${escapeHtml(meta)}</div>
          </div>
          <div class="row-size">${entry.type === "file" ? formatBytes(entry.size) : ""}</div>
        </div>
      `;
    })
    .join("");

  $("fileBrowser").innerHTML = rows || '<div class="empty-state"><p>这个目录是空的</p></div>';

  document.querySelectorAll(".browser-row").forEach((row) => {
    row.addEventListener("click", () => handleBrowserRowClick(row));
    row.addEventListener("dblclick", () => handleBrowserRowOpen(row));
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter") handleBrowserRowOpen(row);
      else if (event.key === " ") { event.preventDefault(); handleBrowserRowClick(row); }
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
    loadFileBrowser(path).catch((error) => toast(error.message, "error"));
  } else {
    choosePath(path);
  }
}

function choosePath(path) {
  selectedPath = path;
  $("sourcePath").value = path;
  $("jobName").value = path.split("/").filter(Boolean).pop() || "";
  renderFileBrowser();
  toast(`已选择: ${path.split("/").pop()}`);
}

// ===== Collections =====
async function loadCollections() {
  collections = await request("/collections");
  renderCollections();
}

function renderCollections() {
  $("collectionCount").textContent = collections.length ? `${collections.length} 个` : "";
  $("collections").innerHTML =
    collections
      .map(
        (item) => `
          <div class="collection-item ${selected && selected.id === item.id ? "active" : ""}" data-id="${item.id}">
            <div class="coll-icon">
              <svg viewBox="0 0 20 20" fill="currentColor"><path d="M7 3a1 1 0 000 2h6a1 1 0 100-2H7zM4 7a1 1 0 011-1h10a1 1 0 110 2H5a1 1 0 01-1-1zM2 11a2 2 0 012-2h12a2 2 0 012 2v4a2 2 0 01-2 2H4a2 2 0 01-2-2v-4z"/></svg>
            </div>
            <div class="coll-info">
              <div class="coll-name" title="${escapeAttr(item.name)}">${escapeHtml(item.name)}</div>
              <div class="coll-meta">${item.episode_count} 个视频 · <span class="badge badge-${statusClass(item.status)}">${escapeHtml(item.status)}</span></div>
            </div>
            <button class="coll-remove" data-id="${item.id}" title="移除">×</button>
          </div>
        `
      )
      .join("") || '<div class="empty-state"><p>还没有已分析合集</p></div>';

  document.querySelectorAll(".collection-item").forEach((el) => {
    el.addEventListener("click", (e) => {
      if (e.target.closest(".coll-remove")) return;
      selectCollection(el.dataset.id);
    });
  });
  document.querySelectorAll(".coll-remove").forEach((btn) => {
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      deleteCollectionById(btn.dataset.id);
    });
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

function renderCollectionDetail() {
  const rows = selected.video_files
    .map(
      (video, index) => `
        <div class="file-row">
          <div>${String(index + 1).padStart(3, "0")}</div>
          <div class="truncate" title="${escapeAttr(video.filename)}">${escapeHtml(video.episode_title)}</div>
          <div>${escapeHtml(trackSummary(video.audio_tracks))}</div>
          <div>${formatDuration(video.duration)}</div>
        </div>
      `
    )
    .join("");

  $("detail").className = "card detail-card";
  $("detail").innerHTML = `
    <div class="detail-header">
      <div>
        <h2 style="font-size:16px;font-weight:600;">${escapeHtml(selected.name)}</h2>
        <p class="text-muted text-sm">${escapeHtml(selected.source_path)} · ${selected.episode_count} 个视频</p>
      </div>
      <div class="detail-actions">
        <button id="rescanCollectionBtn" class="btn btn-secondary btn-sm">重新分析</button>
        <button id="deleteCollectionBtn" class="btn btn-danger btn-sm">移除合集</button>
      </div>
    </div>
    <div class="file-list">
      <div class="file-row header">
        <div>序号</div><div>标题</div><div>可用音轨</div><div>时长</div>
      </div>
      ${rows || '<div class="empty-state"><p>没有符合过滤条件的视频文件</p></div>'}
    </div>
    <pre class="output-preview">${escapeHtml(outputPreview())}</pre>
  `;

  $("rescanCollectionBtn").addEventListener("click", () => detectSource(selected.source_path));
  $("deleteCollectionBtn").addEventListener("click", deleteSelectedCollection);
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

// ===== Jobs =====
async function loadJobs() {
  jobs = await request("/extract/jobs");
  renderJobs();
}

function renderJobs() {
  $("jobSummary").textContent = jobs.length ? `${jobs.length} 个任务` : "";
  $("jobs").innerHTML = jobs
    .map(
      (job) => `
        <div class="job-item" data-id="${job.id}">
          <div class="job-progress-ring">
            ${progressRingSVG(job.progress, statusColor(job.status))}
          </div>
          <div class="job-info">
            <div class="job-name" title="${escapeAttr(job.name || job.id)}">${escapeHtml(job.name || job.id.slice(0, 8))}</div>
            <div class="job-meta">成功 ${job.success_count} / 失败 ${job.failure_count} · <span class="badge badge-${statusClass(job.status)}">${escapeHtml(job.status)}</span></div>
          </div>
        </div>
      `
    )
    .join("") || '<div class="empty-state"><p>暂无任务</p></div>';

  document.querySelectorAll(".job-item").forEach((el) => {
    el.addEventListener("click", () => selectJob(el.dataset.id));
  });
}

function progressRingSVG(percent, color) {
  const r = 16;
  const c = 2 * Math.PI * r;
  const offset = c - (percent / 100) * c;
  return `
    <svg viewBox="0 0 40 40" width="40" height="40">
      <circle cx="20" cy="20" r="${r}" fill="none" stroke="var(--bg-inset)" stroke-width="4"/>
      <circle cx="20" cy="20" r="${r}" fill="none" stroke="${color}" stroke-width="4"
              stroke-dasharray="${c}" stroke-dashoffset="${offset}"
              stroke-linecap="round" transform="rotate(-90 20 20)"/>
      <text x="20" y="20" text-anchor="middle" dominant-baseline="central"
            font-size="9" font-weight="600" fill="var(--text-primary)">${percent}%</text>
    </svg>
  `;
}

async function selectJob(id) {
  selected = null;
  selectedJob = await request(`/extract/jobs/${id}`);
  renderCollections();
  renderJobs();
  renderJobModal();
  openModal("jobModal");
}

function renderJobModal() {
  $("jobModalTitle").textContent = selectedJob.name || "音频提取任务";
  $("jobModalMeta").textContent = `${selectedJob.source_path || "未知源路径"} · ${selectedJob.status}`;

  const rows = selectedJob.items
    .map(
      (item) => `
        <div class="job-row">
          <div><span class="badge badge-${statusClass(item.status)}">${escapeHtml(item.status)}</span></div>
          <div class="truncate" title="${escapeAttr(item.source_path)}">${escapeHtml(item.title || item.source_path)}</div>
          <div class="output-cell">
            <span class="truncate" title="${escapeAttr(item.output_path || item.error_message || "")}">${escapeHtml(item.output_path || item.error_message || "")}</span>
            ${item.status === "completed" && item.output_path ? `<button class="btn btn-ghost btn-sm play-output" data-item-id="${escapeAttr(item.id)}">▶ 播放</button>` : ""}
          </div>
        </div>
      `
    )
    .join("");

  $("jobModalBody").innerHTML = `
    <div class="summary">
      <div class="summary-item"><div class="stat-value">${selectedJob.progress}%</div><div class="stat-label">进度</div></div>
      <div class="summary-item"><div class="stat-value">${selectedJob.total_count}</div><div class="stat-label">总数</div></div>
      <div class="summary-item"><div class="stat-value">${selectedJob.success_count}</div><div class="stat-label">成功</div></div>
      <div class="summary-item"><div class="stat-value">${selectedJob.failure_count}</div><div class="stat-label">失败</div></div>
    </div>
    <div class="progress-bar"><div class="progress-fill" style="width:${selectedJob.progress}%"></div></div>
    <p class="text-muted text-sm" style="margin-bottom:12px">${escapeHtml(selectedJob.current_file || selectedJob.error_message || "")}</p>
    <div class="file-list">
      <div class="job-row header"><div>状态</div><div>文件</div><div>输出 / 错误</div></div>
      ${rows || '<div class="empty-state"><p>暂无任务明细</p></div>'}
    </div>
    ${renderJobAccelerationInfo(selectedJob.summary)}
    <pre class="output-preview">${escapeHtml(JSON.stringify(selectedJob.summary || {}, null, 2))}</pre>
  `;

  document.querySelectorAll(".play-output").forEach((btn) => {
    btn.addEventListener("click", () => playOutputAudio(btn.dataset.itemId));
  });
}

// ===== Core Actions =====
async function ensureSelectedCollection() {
  const sourcePath = $("sourcePath").value.trim();
  if (!sourcePath) throw new Error("请先选择文件夹或视频");
  if (selected && (selected.source_path === sourcePath || selected.video_files.some((v) => v.filepath === sourcePath))) {
    return selected;
  }
  showLoading("正在分析选中项", "正在扫描视频并解析音轨…");
  try {
    const result = await request("/scan/start", {
      method: "POST",
      body: JSON.stringify({ source_paths: [sourcePath] }),
    });
    await loadCollections();
    if (!result.collections.length) throw new Error("没有找到符合过滤条件的视频文件");
    await selectCollection(result.collections[0].id);
    toast(
      `分析完成: ${result.files_found} 个视频${result.warnings.length ? "，部分文件已过滤" : ""}`,
      result.warnings.length ? "warning" : "success"
    );
    return selected;
  } finally {
    hideLoading();
  }
}

async function detectSource(forcedPath = null) {
  try {
    if (forcedPath) $("sourcePath").value = forcedPath;
    await ensureSelectedCollection();
  } catch (error) {
    toast(error.message, "error");
  }
}

async function previewSelected() {
  try {
    const collection = await ensureSelectedCollection();
    if (!collection || !collection.video_files.length) throw new Error("请先分析选中项");
    const video = collection.video_files[0];
    const track = Number($("trackIndex").value);
    const start = Number($("trimStart").value || 0);
    const audio = $("previewAudio");
    audio.src = `${api}/preview/${video.id}?track=${track}&duration=10&start=${start}&_=${Date.now()}`;
    audio.classList.remove("hidden");
    await audio.play().catch(() => {});
    toast(`试听: ${video.filename}`);
  } catch (error) {
    toast(error.message, "error");
  }
}

async function playOutputAudio(itemId) {
  try {
    if (!selectedJob) throw new Error("请先选择一个任务");
    const item = selectedJob.items.find((e) => e.id === itemId);
    const audio = $("previewAudio");
    audio.src = `${api}/extract/jobs/${selectedJob.id}/items/${itemId}/audio?_=${Date.now()}`;
    audio.classList.remove("hidden");
    await audio.play().catch(() => {});
    toast(`播放: ${item ? item.title || item.output_path : itemId}`);
  } catch (error) {
    toast(error.message, "error");
  }
}

async function extractSelected() {
  try {
    const collection = await ensureSelectedCollection();
    if (!collection) throw new Error("请先分析选中项");
    showLoading("正在创建任务", "任务已提交，正在初始化…");
    const job = await request("/extract", {
      method: "POST",
      body: JSON.stringify({
        collection_id: collection.id,
        job_name: $("jobName").value.trim() || collection.name,
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
    toast("任务已创建", "success");
  } catch (error) {
    toast(error.message, "error");
  } finally {
    hideLoading();
  }
}

function startPolling(jobId) {
  if (polling) clearInterval(polling);
  polling = setInterval(async () => {
    const job = await request(`/extract/jobs/${jobId}`);
    selectedJob = job;
    await loadJobs();
    renderJobModal();
    if (["completed", "failed", "cancelled"].includes(job.status)) {
      clearInterval(polling);
      polling = null;
      hideLoading();
      toast(`任务结束: 成功 ${job.success_count}，失败 ${job.failure_count}`, job.failure_count ? "warning" : "success");
    }
  }, 1200);
}

// ===== Delete Actions =====
async function deleteSelectedCollection() {
  if (!selected) return;
  deleteCollectionById(selected.id);
}

function findCollection(id) {
  return collections.find((item) => item.id === id) || null;
}

async function deleteCollectionById(id) {
  const collection = findCollection(id);
  if (!collection) return;
  confirmDialog("移除已分析合集", `将从列表中移除「${collection.name}」，历史任务会保留。`, "确认移除", async () => {
    showLoading("正在移除合集", "正在清理分析记录…");
    try {
      await request(`/collections/${collection.id}`, { method: "DELETE" });
      if (selected && selected.id === collection.id) {
        selected = null;
        $("detail").className = "card detail-card empty";
        $("detail").innerHTML = '<div class="empty-state"><svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M24 8v32M8 24h32"/></svg><p>选择文件夹并分析以查看合集详情</p></div>';
      }
      await loadCollections();
      toast("已移除合集", "success");
    } catch (error) {
      toast(error.message, "error");
    } finally {
      hideLoading();
    }
  });
}

async function deleteSelectedJob() {
  if (!selectedJob) return;
  confirmDialog("删除任务", `将永久删除任务「${selectedJob.name || selectedJob.id.slice(0, 8)}」。`, "确认删除", async () => {
    showLoading("正在删除任务", "正在清理任务记录…");
    try {
      await request(`/extract/jobs/${selectedJob.id}`, { method: "DELETE" });
      selectedJob = null;
      closeModal("jobModal");
      await loadJobs();
      toast("任务已删除", "success");
    } catch (error) {
      toast(error.message, "error");
    } finally {
      hideLoading();
    }
  });
}

// ===== Helpers =====
function collectTracks(collection) {
  const first = collection && collection.video_files.find((v) => v.audio_tracks.length);
  if (!first) return [{ index: 0, label: "默认音轨" }];
  return first.audio_tracks.map((track) => ({
    index: track.index,
    label: `${track.language_full || "未知"} · ${track.codec || "audio"} · ${track.channels || "?"}ch · #${track.index}`,
  }));
}

function trackSummary(tracks) {
  if (!tracks.length) return "未解析";
  return tracks.map((t) => `${t.language_full || "未知"} ${t.codec || ""} #${t.index}`).join(" / ");
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

function previewPadding(total) {
  const configured = $("paddingDigits").value || (settings && settings.padding_digits) || "auto";
  if (configured !== "auto") return Math.max(Number(configured) || 3, 1);
  return total < 1000 ? 3 : 4;
}

function statusClass(value) {
  return { completed: "success", scanned: "success", processing: "info", queued: "info", failed: "danger", error: "danger", cancelled: "muted" }[value] || "muted";
}

function statusColor(value) {
  return { completed: "var(--success)", scanned: "var(--success)", processing: "var(--accent)", queued: "var(--accent)", failed: "var(--danger)", error: "var(--danger)" }[value] || "var(--text-muted)";
}

function csv(value) {
  return value.split(",").map((s) => s.trim()).filter(Boolean);
}

function formatDuration(value) {
  if (!value) return "-";
  const m = Math.floor(value / 60);
  const s = Math.floor(value % 60);
  return `${m}:${String(s).padStart(2, "0")}`;
}

function formatBytes(value) {
  if (!value) return "";
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`;
  return `${(value / 1024 / 1024 / 1024).toFixed(1)} GB`;
}

function escapeHtml(value) {
  return String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#039;" })[c]);
}

function escapeAttr(value) {
  return escapeHtml(value).replace(/"/g, "&quot;");
}

// ===== Event Bindings =====
$("scanBtn").addEventListener("click", () => detectSource());
$("previewBtn").addEventListener("click", previewSelected);
$("extractBtn").addEventListener("click", extractSelected);
$("saveSettingsBtn").addEventListener("click", saveSettings);
$("deleteJobBtn").addEventListener("click", deleteSelectedJob);
$("redetectBtn").addEventListener("click", redetectAcceleration);

$("hardwareAcceleration").addEventListener("change", () => { if (accelerationInfo) renderAccelerationPanel(); });
$("hardwareAccelerationFallback").addEventListener("change", () => { if (accelerationInfo) renderAccelerationPanel(); });

$("refreshFilesBtn").addEventListener("click", () => loadFileBrowser($("browserPath").value).catch((e) => toast(e.message, "error")));
$("openPathBtn").addEventListener("click", () => loadFileBrowser($("browserPath").value).catch((e) => toast(e.message, "error")));
$("parentDirBtn").addEventListener("click", () => browserState && browserState.parent && loadFileBrowser(browserState.parent).catch((e) => toast(e.message, "error")));

$("browserPath").addEventListener("keydown", (e) => {
  if (e.key === "Enter") loadFileBrowser($("browserPath").value).catch((err) => toast(err.message, "error"));
});

document.querySelectorAll(".nav-tab").forEach((btn) => {
  btn.addEventListener("click", () => showView(btn.dataset.view));
});

document.querySelectorAll("[data-close-modal]").forEach((btn) => {
  btn.addEventListener("click", () => closeModal(btn.dataset.closeModal));
});

$("confirmActionBtn").addEventListener("click", async () => {
  closeModal("confirmModal");
  if (confirmHandler) {
    const action = confirmHandler;
    confirmHandler = null;
    await action();
  }
});

// ===== Init =====
loadSettings()
  .then(loadAccelerationInfo)
  .then(() => loadFileBrowser())
  .then(loadCollections)
  .then(loadJobs)
  .then(() => showView(activeView))
  .catch((error) => toast(error.message, "error"));
