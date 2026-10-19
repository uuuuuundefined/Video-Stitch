/* 产品交互原型：状态仅保存在内存；与真实媒体引擎的边界见 README.md。 */
(() => {
  'use strict';

  const root = document.getElementById('vs-terminal');
  const q = (selector) => root.querySelector(selector);
  const qa = (selector) => Array.from(root.querySelectorAll(selector));
  const settings = { transition: 'none', ratio: 'follow-b' };
  const initialA = [
    { id: 'a1', name: '城市漫游.mp4', seconds: 8, size: 18.4, w: 1080, h: 1920 },
    { id: 'a2', name: '周末片段.mp4', seconds: 12, size: 26.8, w: 1920, h: 1080 },
  ];
  const initialB = [
    { id: 'b1', name: '海边日落.mp4', seconds: 24, size: 52.6, w: 1080, h: 1920 },
    { id: 'b2', name: '山间公路.mp4', seconds: 18, size: 39.2, w: 1920, h: 1080 },
  ];
  const labels = {
    pending: 'WAIT · 待导出', preflight: 'CHECK · 预检中', blocked: 'BLOCK · 输入受阻',
    active: 'RUN · 导出中', validating: 'CHECK · 检查结果', done: 'DONE · 已完成',
    failed: 'ERR · 失败', canceled: 'STOP · 已取消', interrupted: 'WAIT · 上次中断',
  };
  let groups = { a: initialA.map((v) => ({ ...v })), b: initialB.map((v) => ({ ...v })) };
  let jobs = [];
  let phase = 'ready';
  let pauseRequested = false;
  let timer = null;
  let folder = '桌面 / 视频拼接_2026-10-17_153045';
  let customFolder = false;
  let dialogAction = null;
  let probeId = 0;
  let recordExists = false;
  const fileIds = new WeakMap();
  const busy = () => ['running', 'pausing', 'paused', 'interrupted'].includes(phase);
  const esc = (value) => String(value).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const basename = (name) => name.replace(/\.[^.]+$/, '');
  const duration = (seconds) => Number.isFinite(seconds)
    ? `${String(Math.floor(seconds / 60)).padStart(2, '0')}:${String(Math.floor(seconds % 60)).padStart(2, '0')}` : '暂无法估算';

  // 由 B 的显示短边计算画布，并保持所选比例及偶数尺寸。
  function dimensions(b) {
    if (settings.ratio === 'follow-b') return `${b.w} × ${b.h}`;
    const [x, y] = settings.ratio.split(':').map(Number);
    const step = Math.max(2, 2 * Math.round(Math.min(b.w, b.h) / (2 * Math.min(x, y))));
    return `${x * step} × ${y * step}`;
  }

  // 创建完整笛卡尔积；异常组合保留在队列中，不静默过滤。
  function rebuild() {
    stopTimer();
    const names = new Map();
    jobs = groups.b.flatMap((b) => groups.a.map((a) => {
      const stem = `${basename(a.name)}__${basename(b.name)}`;
      const n = (names.get(stem) || 0) + 1;
      names.set(stem, n);
      const issue = a.error || b.error || '';
      return {
        id: `${a.id}-${b.id}`, a, b,
        name: `${stem}${n > 1 ? ` (${n})` : ''}.mp4`,
        seconds: Number.isFinite(a.seconds) && Number.isFinite(b.seconds) ? a.seconds + b.seconds : null,
        resolution: b.w && b.h ? dimensions(b) : '待确认',
        status: issue ? 'blocked' : a.loading || b.loading ? 'preflight' : 'pending',
        progress: 0, size: null, issue,
      };
    }));
    phase = 'ready';
    pauseRequested = false;
    recordExists = false;
  }

  // 为长文件名提供本地提示，不依赖图标、字体或浮层库。
  function syncTitles() {
    for (const element of qa('[data-tooltip]')) element.title = element.dataset.tooltip;
  }

  // 展示输入元数据，不创建缩略图或复制文件。
  function renderFiles() {
    for (const g of ['a', 'b']) {
      q(`[data-files="${g}"]`).innerHTML = groups[g].map((v) => {
        const meta = v.loading ? '正在读取视频信息…' : v.error || `${duration(v.seconds)} · ${v.size.toFixed(1)} MB · ${v.w} × ${v.h}`;
        return `<div class="vs-file"><span class="vs-file-icon" aria-hidden="true">├─</span><div class="vs-file-info"><span class="vs-file-name" data-tooltip="${esc(v.name)}">${esc(v.name)}</span><span class="vs-file-meta ${v.error ? 'vs-file-error' : ''}">${esc(meta)}</span></div><button type="button" class="vs-icon-button" aria-label="移除 ${esc(v.name)}" data-action="remove" data-group="${g}" data-id="${esc(v.id)}" ${busy() ? 'disabled' : ''}>×</button></div>`;
      }).join('');
      const invalid = groups[g].filter((v) => v.error).length;
      q(`[data-summary="${g}"]`).textContent = groups[g].length
        ? `${groups[g].length} 个视频 · ${groups[g].reduce((sum, v) => sum + v.size, 0).toFixed(1)} MB${invalid ? ` · ${invalid} 个异常` : ''}` : '尚未添加视频';
    }
  }

  // 展示当前阶段及已确认结果，编码 100% 仍保留检查阶段。
  function renderJobs() {
    q('[data-jobs]').innerHTML = jobs.map((j, i) => {
      const actions = j.status === 'done'
        ? `<button type="button" class="vs-text-button" data-action="open-video" data-id="${esc(j.id)}">打开视频</button>`
        : ['failed', 'blocked'].includes(j.status)
          ? `<button type="button" class="vs-text-button" data-action="error" data-id="${esc(j.id)}">原因${j.status === 'failed' ? ' / 重试' : ''}</button>` : '';
      const progress = j.status === 'blocked' ? '—' : `${Math.floor(j.progress)}%`;
      return `<tr data-job-state="${j.status}"><td class="vs-index">${String(i + 1).padStart(2, '0')}</td><td><span class="vs-output-name" data-tooltip="${esc(j.name)}">${esc(j.name)}</span><span class="vs-row-sub" data-tooltip="${esc(j.a.name)} ＋ ${esc(j.b.name)}">${esc(j.a.name)} ＋ ${esc(j.b.name)}</span>${actions ? `<div class="vs-row-actions">${actions}</div>` : ''}</td><td data-label="时长">${j.status === 'done' ? '' : '预计 '}${duration(j.seconds)}</td><td data-label="大小">${j.size !== null ? `${j.size.toFixed(1)} MB` : '暂无法估算'}</td><td data-label="分辨率">${j.resolution}</td><td><div class="vs-row-status"><span>${labels[j.status]}</span></div><div class="vs-progress" role="progressbar" aria-label="${esc(j.name)} 导出进度" aria-valuemin="0" aria-valuemax="100" aria-valuenow="${Math.floor(j.progress)}"><span aria-hidden="true">[${'█'.repeat(Math.floor(j.progress / 10))}${'·'.repeat(10 - Math.floor(j.progress / 10))}]</span><span class="term-progress-percent">${progress}</span></div></td></tr>`;
    }).join('');
    q('.vs-table').hidden = !jobs.length;
    q('[data-empty]').hidden = !!jobs.length;
  }

  // 将当前任务快照映射为单页控件与性能显示。
  function render() {
    const done = jobs.filter((j) => j.status === 'done').length;
    const failed = jobs.filter((j) => j.status === 'failed').length;
    const blocked = jobs.filter((j) => j.status === 'blocked').length;
    const canceled = jobs.filter((j) => j.status === 'canceled').length;
    const runnable = jobs.filter((j) => j.status === 'pending').length;
    const exporting = ['running', 'pausing'].includes(phase);
    const total = jobs.length ? jobs.reduce((sum, j) => sum + j.progress, 0) / jobs.length : 0;
    qa('[data-transition]').forEach((button) => {
      button.setAttribute('aria-pressed', String(button.dataset.transition === settings.transition));
      button.disabled = busy();
    });
    q('[data-ratio]').value = settings.ratio;
    q('[data-ratio]').disabled = busy();
    qa('[data-action="add"],[data-action="clear"],[data-action="remove"],[data-action="folder"],[data-action="recover"]').forEach((button) => { button.disabled = busy(); });
    qa('[data-drop]').forEach((zone) => zone.classList.toggle('is-locked', busy()));
    q('[data-count-line]').textContent = `${groups.a.length} × ${groups.b.length} = ${jobs.length} 个视频${blocked ? ` · ${blocked} 个受阻` : ''}`;
    q('[data-phase]').textContent = ({ ready: blocked ? `${runnable} 个可导出 · ${blocked} 个受阻` : jobs.length ? '准备就绪' : '等待添加', running: '正在导出', pausing: '完成当前视频后暂停', paused: '队列已暂停', interrupted: '上次中断', complete: failed ? `${failed} 个失败` : blocked ? `${blocked} 个受阻` : '全部完成', canceled: '已取消剩余任务' })[phase];
    q('[data-setting-note]').textContent = `${settings.transition === 'boundary-fade' ? '叠化 0.5 秒 · 总时长不变 · ' : ''}等比裁边铺满 · 分辨率、帧率跟随 B`;
    q('[data-path]').textContent = folder;
    q('[data-action="pause"]').hidden = !exporting;
    q('[data-action="pause"]').disabled = pauseRequested;
    q('[data-action="pause"]').textContent = pauseRequested ? '等待当前视频完成' : '暂停队列';
    q('[data-action="cancel"]').hidden = phase === 'ready' || !jobs.some((j) => !['done', 'canceled'].includes(j.status));
    const start = q('[data-action="start"]');
    start.disabled = !jobs.length || exporting || groups.a.concat(groups.b).some((v) => v.loading) || phase === 'ready' && !runnable;
    q('[data-primary-label]').textContent = exporting ? '正在导出…'
      : phase === 'paused' || phase === 'interrupted' ? '继续导出'
        : phase === 'canceled' ? '新建批次'
          : phase === 'complete' ? failed ? `重试失败 · ${failed} 个` : blocked ? '查看受阻原因' : '打开文件夹'
            : `开始导出 · ${runnable} 个视频`;
    q('.vs-overall').hidden = phase === 'ready';
    q('[data-overall-label]').textContent = `已完成 ${done} / ${jobs.length}${failed ? ` · ${failed} 个失败` : ''}${blocked ? ` · ${blocked} 个受阻` : ''}${canceled ? ` · ${canceled} 个已取消` : ''}`;
    q('[data-overall-detail]').textContent = exporting ? '正在处理并检查结果'
      : phase === 'paused' ? '当前视频已完成，其余任务等待继续'
        : phase === 'interrupted' ? '中断视频将从头处理，完成项保留'
          : phase === 'canceled' ? '已取消任务不会恢复；已完成视频保留'
            : failed || blocked ? '检查原因后处理未完成项' : '导出完成';
    q('[data-total-bar]').textContent = `[${'█'.repeat(Math.floor(total / 5))}${'·'.repeat(20 - Math.floor(total / 5))}] ${Math.floor(total)}%`;
    q('.vs-total-progress').setAttribute('aria-valuenow', String(Math.floor(total)));
    q('[data-runtime]').textContent = exporting ? '自动加速 · 正在处理' : phase === 'interrupted' ? '等待恢复任务' : '本地处理 · 就绪';
    q('[data-cpu]').textContent = exporting ? '32%' : '8%';
    q('[data-memory]').textContent = exporting ? '624 MB' : '186 MB';
    q('[data-system-memory]').textContent = exporting ? '10.2 / 32 GB' : '9.4 / 32 GB';
    q('[data-gpu]').textContent = exporting ? '编码 24%' : '暂不可用';
    q('[data-speed]').textContent = exporting ? '1.8×' : '—';
    q('[data-eta]').textContent = exporting ? '估算中' : phase === 'paused' ? '已暂停' : '—';
    renderFiles();
    renderJobs();
    syncTitles();
  }

  // 将用户触发的反馈限定于当前界面，不写日志。
  function notice(text) { q('[data-notice]').textContent = text; }

  // 在同一个原生对话框内呈现确认与路径输入。
  function modal(title, copy, confirm, action, field = null) {
    q('#vs-terminal-dialog-title').textContent = title;
    q('[data-dialog-copy]').textContent = copy;
    q('[data-dialog-confirm]').textContent = confirm;
    q('.term-dialog-field').hidden = !field;
    if (field) {
      q('[data-field-label]').textContent = field.label;
      q('[data-dialog-input]').value = field.value;
    }
    dialogAction = action;
    q('.vs-dialog').showModal();
  }

  // 停止当前交互调度器，避免暂停、重试或新批次残留多个定时器。
  function stopTimer() { if (timer !== null) clearInterval(timer); timer = null; }

  // 仅未完成、失败或阻断项使本批次需要恢复记录。
  function updateRecordState() {
    recordExists = jobs.some((j) => !['done', 'canceled'].includes(j.status));
  }

  // 完成校验后提交单项状态；原型只更新内存，不输出视频。
  function finish(job) {
    job.status = 'done';
    job.progress = 100;
    job.size = Number(((job.a.size + job.b.size) * 0.74).toFixed(1));
  }

  // 按处理→校验→成功推进；暂停必须等在途任务完成。
  function tick() {
    if (!['running', 'pausing'].includes(phase)) return;
    let current = jobs.find((j) => ['active', 'validating'].includes(j.status));
    if (current?.status === 'validating') {
      finish(current);
      current = null;
      if (pauseRequested && jobs.some((j) => j.status === 'pending')) {
        phase = 'paused';
        pauseRequested = false;
        stopTimer();
        updateRecordState();
        notice('队列已暂停。当前视频已完成，其余任务等待继续。');
        render();
        return;
      }
    } else if (current) {
      current.progress = Math.min(100, current.progress + 8);
      if (current.progress === 100) current.status = 'validating';
    } else {
      current = jobs.find((j) => j.status === 'pending');
      if (current) current.status = 'active';
    }
    if (!jobs.some((j) => ['pending', 'active', 'validating'].includes(j.status))) {
      phase = 'complete';
      pauseRequested = false;
      stopTimer();
      notice(jobs.some((j) => ['failed', 'blocked'].includes(j.status)) ? '本批次已结束，请检查未完成项。' : `已完成 ${jobs.filter((j) => j.status === 'done').length} 个视频。`);
    }
    updateRecordState();
    render();
  }

  // 开始当前批次；沿用默认时间目录及用户自选最终目录逻辑。
  function schedule() {
    if (!jobs.some((j) => j.status === 'pending')) return;
    if (!customFolder && phase === 'ready') {
      const d = new Date();
      const p = (n) => String(n).padStart(2, '0');
      folder = `桌面 / 视频拼接_${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
    }
    phase = 'running';
    pauseRequested = false;
    updateRecordState();
    stopTimer();
    timer = setInterval(tick, 500);
    notice('正在导出；请保持源文件和保存位置可用。');
    render();
  }

  // 开始前集中提示异常，重试与恢复不包含成功或取消项。
  function start() {
    if (!jobs.length || ['running', 'pausing'].includes(phase)) return;
    if (phase === 'canceled') { rebuild(); render(); notice('新批次已准备，请确认设置后开始导出。'); return; }
    if (phase === 'complete') {
      const failed = jobs.filter((j) => j.status === 'failed');
      if (failed.length) for (const job of failed) { job.status = 'pending'; job.progress = 0; job.issue = ''; }
      else if (jobs.some((j) => j.status === 'blocked')) { showIssues(); return; }
      else { showOutput(); return; }
    }
    for (const job of jobs.filter((j) => j.status === 'interrupted')) { job.status = 'pending'; job.progress = 0; }
    const blocked = jobs.filter((j) => j.status === 'blocked');
    if (blocked.length) {
      modal('部分组合无法导出', `共 ${jobs.length} 个组合，其中 ${blocked.length} 个受阻。\n${blocked.slice(0, 3).map((j) => `${j.name}：${j.issue}`).join('\n')}\n是否先导出其余正常组合？`, '只导出正常组合', schedule);
    } else schedule();
  }

  // 明确放弃所有剩余项，取消项不能被继续或恢复。
  function cancelRemaining() {
    stopTimer();
    for (const job of jobs) if (job.status !== 'done') { job.status = 'canceled'; job.progress = 0; }
    phase = 'canceled';
    pauseRequested = false;
    updateRecordState();
    notice('剩余任务已取消，不再恢复；已完成视频保留。');
    render();
  }

  // 通过浏览器只读取元数据，始终释放对象 URL。
  function inspect(file, g) {
    return new Promise((resolve) => {
      const id = fileIds.get(file) || `local-${++probeId}`;
      fileIds.set(file, id);
      const item = { id, name: file.name, size: file.size / 1e6, seconds: null, w: 0, h: 0, loading: true };
      groups[g].push(item);
      rebuild();
      render();
      const video = document.createElement('video');
      const url = URL.createObjectURL(file);
      let ended = false;
      // 读取结束时释放解码资源，并只更新当前输入计划。
      function complete(error) {
        if (ended) return;
        ended = true;
        clearTimeout(timeout);
        if (error) item.error = error;
        else if (Number.isFinite(video.duration) && video.duration > 0 && video.videoWidth > 0 && video.videoHeight > 0) {
          item.seconds = video.duration; item.w = video.videoWidth; item.h = video.videoHeight;
        } else item.error = '无法读取视频信息';
        item.loading = false;
        video.removeAttribute('src'); video.load(); URL.revokeObjectURL(url);
        resolve();
      }
      const timeout = setTimeout(() => complete('视频信息读取超时'), 10000);
      video.preload = 'metadata';
      video.onloadedmetadata = () => complete();
      video.onerror = () => complete('无法读取视频信息，请检查文件是否可用');
      video.src = url;
    });
  }

  // 同组只对能证明相同的 File 引用去重，不误删同名同大小文件。
  async function importFiles(files, g) {
    if (busy()) { notice('当前批次已锁定，请完成或取消后再修改视频。'); return; }
    let imported = 0;
    let duplicates = 0;
    for (const file of Array.from(files)) {
      if (busy()) break;
      const id = fileIds.get(file);
      if (id && groups[g].some((v) => v.id === id)) { duplicates++; continue; }
      await inspect(file, g);
      imported++;
    }
    rebuild();
    render();
    notice(`添加 ${imported} 个视频${duplicates ? `，跳过 ${duplicates} 个重复文件` : ''}。`);
  }

  // 展示阻断原因，不因浏览器不支持而宣称正式引擎也不支持。
  function showIssues(job = null) {
    const issues = job ? [job] : jobs.filter((j) => ['failed', 'blocked'].includes(j.status));
    const canRetry = job?.status === 'failed';
    modal(canRetry ? '视频导出失败' : '组合无法导出', issues.slice(0, 5).map((j) => `${j.name}\n${j.issue || '保存位置暂时不可写，请检查权限与磁盘空间。'}`).join('\n\n'), canRetry ? '重试此视频' : '知道了', () => {
      if (!canRetry) return;
      job.status = 'pending'; job.progress = 0; job.issue = '';
      schedule();
    });
  }

  // 呈现已完成结果的位置；原型不调用原生系统打开能力。
  function showOutput(job = null) {
    modal(job ? '视频位置' : '保存位置', job ? `${folder} / ${job.name}` : folder, '确定', () => {});
  }

  // 在产品控件之外用 URL 状态选择检查不同生命周期界面。
  function applyScenario(state) {
    stopTimer();
    groups = { a: initialA.map((v) => ({ ...v })), b: initialB.map((v) => ({ ...v })) };
    if (state === 'empty') groups = { a: [], b: [] };
    if (state === 'blocked') groups.a[1].error = 'HDR 视频首版暂不支持';
    rebuild();
    if (['running', 'paused', 'interrupted', 'validating', 'completed', 'failed'].includes(state)) {
      finish(jobs[0]);
      recordExists = true;
      if (state === 'running' || state === 'validating') {
        jobs[1].status = state === 'validating' ? 'validating' : 'active';
        jobs[1].progress = state === 'validating' ? 100 : 46;
        phase = 'running';
      } else if (state === 'paused') phase = 'paused';
      else if (state === 'interrupted') {
        jobs[1].status = 'interrupted'; phase = 'interrupted';
      } else if (state === 'completed' || state === 'failed') {
        jobs.forEach(finish);
        phase = 'complete';
        if (state === 'failed') { jobs[1].status = 'failed'; jobs[1].progress = 0; jobs[1].size = null; jobs[1].issue = '保存位置暂时不可写，请检查权限和空间。'; }
      }
      updateRecordState();
    }
    if (state === 'canceled') cancelRemaining();
    render();
  }

  // 统一处理产品按钮，确认动作只执行一次。
  function handleClick(event) {
    const button = event.target.closest('button');
    if (!button || button.disabled) return;
    const action = button.dataset.action;
    const g = button.dataset.group;
    if (button.hasAttribute('data-dialog-confirm')) {
      const callback = dialogAction; dialogAction = null; q('.vs-dialog').close(); callback?.(); return;
    }
    if (button.dataset.transition) {
      if (busy()) return;
      settings.transition = button.dataset.transition; rebuild(); render(); return;
    }
    if (action === 'add') q(`[data-input="${g}"]`).click();
    if (action === 'remove' && !busy()) { groups[g] = groups[g].filter((v) => v.id !== button.dataset.id); rebuild(); render(); }
    if (action === 'clear' && !busy()) { groups[g] = []; rebuild(); render(); }
    if (action === 'folder' && !busy()) modal('视频保存位置', '默认在桌面按导出时间创建文件夹。\n更改后直接使用所选目录，同名文件自动编号，不覆盖。', '使用此位置', () => {
      const value = q('[data-dialog-input]').value.trim();
      if (value) { folder = value; customFolder = true; render(); notice('保存位置已更新。'); }
    }, { label: '保存位置', value: folder });
    if (action === 'recover' && !busy()) modal('恢复未完成任务', '请选择导出目录中的任务恢复记录（JSON）。\n中断视频从头处理，完成项不重做，取消项不恢复。', '选择恢复记录', () => q('[data-recovery-input]').click());
    if (action === 'start') start();
    if (action === 'pause' && ['running', 'pausing'].includes(phase)) { pauseRequested = true; phase = 'pausing'; if (timer === null) timer = setInterval(tick, 500); notice('将完成当前视频，再暂停后续任务。'); render(); }
    if (action === 'cancel' && phase !== 'ready' && jobs.some((j) => !['done', 'canceled'].includes(j.status))) modal('取消剩余导出？', '取消表示放弃剩余任务，删除对应临时视频，不再恢复。\n已完成的视频保留；全部结束后删除恢复记录。\n如想以后继续，请使用暂停队列或停止并退出。', '取消剩余导出', cancelRemaining);
    if (action === 'error') showIssues(jobs.find((j) => j.id === button.dataset.id));
    if (action === 'open-video') showOutput(jobs.find((j) => j.id === button.dataset.id && j.status === 'done'));
  }

  root.addEventListener('click', handleClick);
  root.addEventListener('change', (event) => {
    if (event.target.matches('[data-ratio]') && !busy()) { settings.ratio = event.target.value; rebuild(); render(); }
    if (event.target.matches('[data-input]')) { importFiles(event.target.files, event.target.dataset.input); event.target.value = ''; }
    if (event.target.matches('[data-recovery-input]')) {
      const file = event.target.files?.[0];
      event.target.value = '';
      if (!file) return;
      modal('恢复未完成任务', `恢复记录：${file.name}\n已完成项保留，中断项从头处理，取消项不恢复。`, '恢复未完成任务', () => {
        applyScenario('interrupted'); notice('任务已载入，确认后可继续导出。');
      });
    }
  });
  for (const zone of qa('[data-drop]')) {
    zone.addEventListener('dragover', (event) => { event.preventDefault(); if (!busy()) zone.classList.add('is-over'); });
    zone.addEventListener('dragleave', () => zone.classList.remove('is-over'));
    zone.addEventListener('drop', (event) => {
      event.preventDefault(); zone.classList.remove('is-over');
      if (Array.from(event.dataTransfer?.items || []).some((item) => item.webkitGetAsEntry?.()?.isDirectory)) { notice('请导入视频文件，不支持文件夹。'); return; }
      if (event.dataTransfer?.files) importFiles(event.dataTransfer.files, zone.dataset.drop);
    });
  }
  // 页面关闭时只释放内存调度器，不保存任何设置或业务状态。
  window.addEventListener('pagehide', stopTimer);
  const scenario = new URLSearchParams(location.hash.slice(1)).get('state');
  const states = ['empty', 'ready', 'running', 'paused', 'interrupted', 'validating', 'completed', 'failed', 'blocked', 'canceled'];
  applyScenario(states.includes(scenario) ? scenario : 'ready');
})();
