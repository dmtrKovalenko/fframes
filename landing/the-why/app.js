const root = document.documentElement;
// Decorative motion follows the system setting; reduced motion keeps the page static.
const motion = !matchMedia('(prefers-reduced-motion: reduce)').matches;
root.classList.add('js-motion');
if (motion) root.classList.add('motion-on');

// Plausible goals. The pageview, scroll depth and time on page are tracked by the script itself.
const track = (name, props) => window.plausible?.(name, props ? { props } : undefined);
const area = el => {
  const section = el.closest('header, footer, section');
  return section?.id || section?.classList[0] || section?.localName || 'page';
};
const trackedOnce = new Set();
const trackOnce = (name, props) => {
  const key = name + JSON.stringify(props ?? {});
  if (trackedOnce.has(key)) return;
  trackedOnce.add(key);
  track(name, props);
};
const observer = new IntersectionObserver(entries => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    entry.target.classList.add('visible');
    const counter = entry.target.querySelector('[data-count]');
    if (counter && motion) {
      const target = Number(counter.dataset.count), start = performance.now();
      function tick(now) {
        const p = Math.min((now - start) / 1400, 1);
        counter.textContent = Math.round(target * (1 - (1-p)**3)).toLocaleString('en-US');
        if (p < 1 && motion) requestAnimationFrame(tick);
        else counter.textContent = target.toLocaleString('en-US');
      }
      requestAnimationFrame(tick);
    }
    observer.unobserve(entry.target);
  }
}, { threshold: 0.12 });
document.querySelectorAll('.reveal').forEach(el => observer.observe(el));
const playlist = 'assets/hls/master.m3u8';
const nativeHls = Boolean(document.createElement('video').canPlayType('application/vnd.apple.mpegurl'));
// hls.js is 540 KB, so it loads only once a player is close to being used.
let hlsLoader = null;
function loadHls() {
  if (nativeHls || window.Hls) return Promise.resolve();
  hlsLoader ??= new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = 'vendor/hls.min.js';
    script.onload = resolve;
    script.onerror = () => { hlsLoader = null; reject(new Error('hls.js failed to load')); };
    document.head.append(script);
  });
  return hlsLoader;
}
// Both players use the same lazy HLS setup and release it when collapsed.
function createPlayback(video, report) {
  let hls = null, initialized = false, pendingTime = null, request = 0;
  video.addEventListener('loadedmetadata', () => {
    if (pendingTime !== null) { video.currentTime = pendingTime; pendingTime = null; }
  });
  async function initialize(current) {
    if (initialized) return true;
    if (!nativeHls) {
      try { await loadHls(); } catch { report('The player could not load. Check your connection and try again.'); return false; }
      if (current !== request) return false;
    }
    video.querySelector('source')?.remove();
    if (nativeHls) {
      video.src = playlist;
    } else if (window.Hls?.isSupported()) {
      hls = new Hls({ autoStartLoad: false, capLevelToPlayerSize: true, maxBufferLength: 20 });
      hls.loadSource(playlist);
      hls.attachMedia(video);
      let recoveries = 0;
      hls.on(Hls.Events.ERROR, (_event, data) => {
        if (!data.fatal) return;
        if (data.type === Hls.ErrorTypes.MEDIA_ERROR && recoveries++ < 1) hls.recoverMediaError();
        else {
          report('Playback interrupted. Close this preview and try again.');
          track('Film error', { reason: data.details });
        }
      });
    } else {
      report('Streaming is not supported in this browser. Try Safari, Chrome, Firefox or Edge.');
      track('Film error', { reason: 'unsupported' });
      return false;
    }
    initialized = true;
    return true;
  }
  video.addEventListener('error', () => {
    if (!initialized || hls) return;
    report('The film could not load. Close this preview and try again.');
    track('Film error', { reason: `media ${video.error?.code ?? 'unknown'}` });
  });
  return {
    async play(time) {
      const current = ++request;
      report('');
      if (!(await initialize(current))) return;
      pendingTime = time;
      if (video.readyState >= 1) { video.currentTime = time; pendingTime = null; }
      hls?.startLoad(time);
      video.play().catch(error => {
        if (current === request && error.name !== 'AbortError') report('Press play in the video controls to start.');
      });
    },
    reset() {
      request++;
      initialized = false;
      pendingTime = null;
      video.pause();
      hls?.destroy();
      hls = null;
      video.removeAttribute('src');
      video.querySelector('source')?.remove();
      video.load();
      report('');
    }
  };
}
const movie = document.querySelector('#movie');
const player = document.querySelector('.player-wrap');
const status = document.querySelector('#player-status');
const closeFilm = document.querySelector('#close-film');
const endScreen = document.querySelector('#player-end');
const filmPlayback = createPlayback(movie, message => { status.textContent = message; });
let activeScene = null;
// Start fetching hls.js while the visitor approaches a player, so pressing play feels instant.
new IntersectionObserver((entries, watcher) => {
  if (!entries.some(entry => entry.isIntersecting)) return;
  watcher.disconnect();
  loadHls().catch(() => {});
}, { rootMargin: '600px 0px' }).observe(player);
document.querySelectorAll('[data-play], [data-scene]').forEach(button => {
  button.addEventListener('pointerenter', () => loadHls().catch(() => {}), { once: true });
});
function showEndScreen(visible) {
  endScreen.hidden = !visible;
  player.classList.toggle('ended', visible);
}
function collapseFilm(restoreFocus = true) {
  filmPlayback.reset();
  showEndScreen(false);
  player.classList.remove('started');
  closeFilm.hidden = true;
  if (restoreFocus) player.querySelector('[data-play]').focus({ preventScroll: true });
}
closeFilm.addEventListener('click', () => collapseFilm());
movie.addEventListener('play', () => {
  showEndScreen(false);
  closeScene(false);
  player.classList.add('started');
  closeFilm.hidden = false;
});
player.addEventListener('keydown', event => {
  if (event.key === 'Escape' && player.classList.contains('started')) collapseFilm();
});
// Watched share of the film, counted from the ranges actually played, so seeking ahead does not count.
const milestones = [25, 50, 75];
movie.addEventListener('timeupdate', () => {
  if (!movie.duration) return;
  let watched = 0;
  for (let i = 0; i < movie.played.length; i++) watched += movie.played.end(i) - movie.played.start(i);
  const percent = watched / movie.duration * 100;
  for (const milestone of milestones) if (percent >= milestone) trackOnce('Film progress', { percent: String(milestone) });
});
movie.addEventListener('ended', () => {
  trackOnce('Film finished');
  if (document.fullscreenElement) document.exitFullscreen?.().catch(() => {});
  showEndScreen(true);
  endScreen.querySelector('button').focus({ preventScroll: true });
});
document.querySelector('#replay-film').addEventListener('click', () => {
  track('Film replay');
  showEndScreen(false);
  filmPlayback.play(0);
});
document.querySelectorAll('[data-play]').forEach(button => button.addEventListener('click', () => {
  track('Film play', { from: player.contains(button) ? 'player' : 'hero' });
  closeScene(false);
  player.classList.add('started');
  closeFilm.hidden = false;
  // Only the hero action navigates. The player itself stays in place.
  if (!player.contains(button)) player.scrollIntoView({ behavior: motion ? 'smooth' : 'instant', block: 'center' });
  filmPlayback.play(Number(button.dataset.play));
}));
function closeScene(restoreFocus = true) {
  if (!activeScene) return;
  const { playback, shell, picture, trigger } = activeScene;
  playback.reset();
  shell.remove();
  picture.classList.remove('is-playing');
  trigger.setAttribute('aria-expanded', 'false');
  activeScene = null;
  if (restoreFocus) trigger.focus({ preventScroll: true });
}
document.querySelectorAll('[data-scene]').forEach(trigger => trigger.addEventListener('click', () => {
  closeScene(false);
  movie.pause();
  const picture = trigger.closest('.scene-picture');
  const shell = document.createElement('div');
  shell.className = 'scene-inline';
  const video = document.createElement('video');
  video.controls = true;
  video.playsInline = true;
  video.preload = 'none';
  video.setAttribute('aria-label', trigger.getAttribute('aria-label').replace('Play scene: ', ''));
  const captions = document.createElement('track');
  Object.assign(captions, { kind: 'captions', src: 'assets/captions.vtt', srclang: 'en', label: 'English' });
  video.append(captions);
  const close = document.createElement('button');
  close.className = 'player-close';
  close.setAttribute('aria-label', 'Close scene preview');
  close.append(document.querySelector('#close-icon').content.cloneNode(true));
  close.addEventListener('click', () => closeScene());
  const message = document.createElement('p');
  message.className = 'scene-error';
  message.setAttribute('role', 'status');
  const playback = createPlayback(video, text => { message.textContent = text; });
  shell.append(video, close, message);
  picture.append(shell);
  picture.classList.add('is-playing');
  trigger.setAttribute('aria-expanded', 'true');
  activeScene = { playback, shell, picture, trigger };
  shell.addEventListener('keydown', event => { if (event.key === 'Escape') closeScene(); });
  const start = Number(trigger.dataset.scene);
  track('Scene play', { scene: video.getAttribute('aria-label') });
  playback.play(start);
  close.focus({ preventScroll: true });
}));
const facts = [...document.querySelectorAll('[data-preview]')];
function selectFact(fact) {
  for (const item of facts) {
    const selected = item === fact;
    item.classList.toggle('is-active', selected);
    item.querySelector('.fact-select').setAttribute('aria-pressed', String(selected));
    const panel = document.querySelector(`#preview-${item.dataset.preview}`);
    panel.classList.toggle('is-active', selected);
    panel.setAttribute('aria-hidden', String(!selected));
    panel.inert = !selected;
  }
}
for (const fact of facts) {
  fact.addEventListener('pointerenter', event => { if (event.pointerType !== 'touch') selectFact(fact); });
  fact.addEventListener('focusin', () => selectFact(fact));
  fact.addEventListener('click', () => {
    selectFact(fact);
    trackOnce('Fact preview', { fact: fact.dataset.preview });
  });
}
const progress = document.querySelector('.reading-progress');
const heroImage = document.querySelector('.hero-image');
let scrollQueued = false;
function updateScroll() {
  progress.style.transform = `scaleX(${scrollY / Math.max(1, document.documentElement.scrollHeight-innerHeight)})`;
  if (motion && scrollY < innerHeight) heroImage.style.translate = `0 ${scrollY * .15}px`;
  else heroImage.style.translate = '0 0';
  scrollQueued = false;
}
addEventListener('scroll', () => { if (!scrollQueued) { scrollQueued = true; requestAnimationFrame(updateScroll); } }, { passive: true });
updateScroll();
// Outbound links. GitHub clicks share the goal name with the homepage.
document.addEventListener('click', event => {
  const link = event.target.closest?.('a[href]');
  if (!link || link.host === location.host) return;
  if (link.host === 'github.com') track('GitHub click', { location: area(link), path: link.pathname });
  else track('Outbound click', { location: area(link), host: link.host });
});

const shareData = {
  title: 'THE WHY',
  text: 'THE WHY, a 98-second film written in Rust and rendered with fframes.',
  url: 'https://fframes.studio/the-why',
};
function flash(button, text) {
  const label = button.querySelector('span') ?? button.firstChild;
  const original = label.textContent;
  label.textContent = text;
  setTimeout(() => { label.textContent = original; }, 1800);
}
document.querySelectorAll('[data-share]').forEach(button => button.addEventListener('click', async () => {
  const from = area(button) === 'film' ? 'end screen' : area(button);
  if (navigator.share) {
    try {
      await navigator.share(shareData);
      track('Share', { method: 'native', location: from });
    } catch {}
    return;
  }
  try {
    await navigator.clipboard.writeText(shareData.url);
    track('Share', { method: 'copy link', location: from });
    flash(button, 'Link copied ');
  } catch {
    flash(button, 'Copy fframes.studio/the-why ');
  }
}));

// Copy the command without the prompt characters.
document.querySelectorAll('.copy').forEach(button => button.addEventListener('click', async () => {
  const text = button.previousElementSibling.textContent.replace(/^\$ /gm, '');
  try {
    await navigator.clipboard.writeText(text);
    track('Copy command', { command: text.includes('skills add') ? 'skill' : 'cargo' });
    button.classList.add('copied');
    button.setAttribute('aria-label', 'Copied');
    setTimeout(() => {
      button.classList.remove('copied');
      button.setAttribute('aria-label', 'Copy command');
    }, 1600);
  } catch {
    button.setAttribute('aria-label', 'Copy failed, select the command instead');
  }
}));

const canvas = document.querySelector('#dust');
const ctx = canvas.getContext('2d');
let w, h;
function resize() {
  w = canvas.clientWidth; h = canvas.clientHeight;
  const dpr = Math.min(devicePixelRatio || 1, 2);
  canvas.width = w * dpr; canvas.height = h * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}
resize();
addEventListener('resize', resize);
const particles = Array.from({length: 38}, () => ({ x: Math.random(), y: Math.random(), r: Math.random()*1.3+.3, speed: Math.random()*.000018+.000007, phase: Math.random()*6 }));
let last = 0, drawing = false;
function draw(now) {
  if (!drawing) return;
  const delta = Math.min(now-last, 40); last = now;
  {
    ctx.clearRect(0, 0, w, h);
    for (const p of particles) {
      p.y -= p.speed * delta;
      if (p.y < 0) p.y = 1;
      ctx.beginPath();
      ctx.fillStyle = `rgba(255,215,155,${.15 + (Math.sin(now*.0007+p.phase)+1)*.2})`;
      ctx.arc(p.x*w+Math.sin(now*.00015+p.phase)*18, p.y*h, p.r, 0, Math.PI*2);
      ctx.fill();
    }
  }
  requestAnimationFrame(draw);
}
// The dust only animates while the hero is on screen.
if (motion) new IntersectionObserver(entries => {
  const visible = entries.some(entry => entry.isIntersecting);
  if (visible && !drawing) { drawing = true; last = performance.now(); requestAnimationFrame(draw); }
  drawing = visible;
}).observe(canvas);

// Slide the original landing-page wordmark into place when it enters view.
const footerMark = document.querySelector('.footer-mark');
if (footerMark) {
  const footerObserver = new IntersectionObserver(entries => {
    if (entries.some(entry => entry.isIntersecting)) {
      footerMark.classList.add('is-visible');
      footerObserver.disconnect();
    }
  }, { threshold: 0.3 });
  footerObserver.observe(footerMark);
}
