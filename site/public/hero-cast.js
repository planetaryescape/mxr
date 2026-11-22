(function () {
  var cast = document.querySelector('.hero-cast');
  if (!cast) return;

  var radios = Array.from(cast.querySelectorAll('.hero-cast-radio'));
  var videos = Array.from(cast.querySelectorAll('.hero-cast-video'));
  var toggle = cast.querySelector('.hero-cast-motion');
  if (!toggle || radios.length !== videos.length) return;

  var reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  var paused = reducedMotion.matches;
  var cycleTimer;
  var cycleDelay = 8000;
  var pointerInside = false;
  var focusInside = false;

  function selectedVideo() {
    var index = radios.findIndex(function (radio) {
      return radio.checked;
    });
    return videos[index];
  }

  function running(video) {
    return !!video && !video.paused && !video.ended && !video.error;
  }

  function label() {
    var next = running(selectedVideo()) ? 'pause' : 'play';
    toggle.textContent = next;
    toggle.setAttribute('aria-label', next.charAt(0).toUpperCase() + next.slice(1) + ' demo');
  }

  function stopCycle() {
    window.clearTimeout(cycleTimer);
    cycleTimer = undefined;
  }

  function canCycle() {
    return !paused && !reducedMotion.matches && !pointerInside && !focusInside && !document.hidden;
  }

  function scheduleCycle() {
    stopCycle();
    if (!canCycle()) return;
    cycleTimer = window.setTimeout(function () {
      var index = radios.findIndex(function (radio) { return radio.checked; });
      radios[(index + 1) % radios.length].checked = true;
      sync();
    }, cycleDelay);
  }

  var attempt = 0;

  function failed(video, token) {
    if (token !== attempt || video !== selectedVideo()) return;
    paused = true;
    label();
    stopCycle();
  }

  // src is assigned on first play, so unselected recordings, and all of them
  // while motion is paused, never hit the network.
  function play(video) {
    if (!video.getAttribute('src')) video.src = video.dataset.src;
    var token = ++attempt;
    try {
      var started = video.play();
      // Browsers older than the promise-returning play() return undefined.
      if (started) started.catch(function () { failed(video, token); });
    } catch (error) {
      failed(video, token);
    }
  }

  function reveal(video) {
    if (!video || !video.dataset.poster) return;
    video.poster = video.dataset.poster;
    delete video.dataset.poster;
    var fallback = video.querySelector('img[data-src]');
    if (fallback) fallback.src = fallback.dataset.src;
  }

  function sync() {
    var active = selectedVideo();
    reveal(active);
    videos.forEach(function (video) {
      if (video === active && !paused) play(video);
      else video.pause();
    });
    label();
    scheduleCycle();
  }

  function setPaused(next) {
    paused = next;
    sync();
  }

  radios.forEach(function (radio) {
    radio.addEventListener('change', sync);
  });

  function track(event) {
    var video = selectedVideo();
    if (event.target !== video) return;
    paused = !running(video);
    label();
  }

  videos.forEach(function (video) {
    ['play', 'pause', 'ended', 'error'].forEach(function (type) {
      video.addEventListener(type, track);
    });
  });

  toggle.addEventListener('click', function () {
    setPaused(!paused);
  });

  cast.addEventListener('pointerenter', function () {
    pointerInside = true;
    stopCycle();
  });

  cast.addEventListener('pointerleave', function () {
    pointerInside = false;
    scheduleCycle();
  });

  cast.addEventListener('focusin', function () {
    focusInside = true;
    stopCycle();
  });

  cast.addEventListener('focusout', function (event) {
    if (cast.contains(event.relatedTarget)) return;
    focusInside = false;
    scheduleCycle();
  });

  document.addEventListener('visibilitychange', scheduleCycle);

  reducedMotion.addEventListener('change', function (event) {
    setPaused(event.matches);
  });

  toggle.hidden = false;
  setPaused(paused);
})();
