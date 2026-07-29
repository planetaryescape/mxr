(function () {
  var cast = document.querySelector('.hero-cast');
  if (!cast) return;

  // Radios and videos are authored in the same order: cli, tui, agent.
  var radios = Array.from(cast.querySelectorAll('.hero-cast-radio'));
  var videos = Array.from(cast.querySelectorAll('.hero-cast-video'));
  var toggle = cast.querySelector('.hero-cast-motion');
  if (!toggle || radios.length !== videos.length) return;

  var reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  var paused = reducedMotion.matches;

  // src is assigned on first play, so unselected recordings, and all of them
  // while motion is paused, never hit the network.
  function play(video) {
    if (!video.getAttribute('src')) video.src = video.dataset.src;
    video.play().catch(function () {});
  }

  function sync() {
    var active = radios.findIndex(function (radio) {
      return radio.checked;
    });
    videos.forEach(function (video, index) {
      if (index === active && !paused) play(video);
      else video.pause();
    });
  }

  function setPaused(next) {
    paused = next;
    toggle.textContent = paused ? 'play' : 'pause';
    toggle.setAttribute('aria-label', (paused ? 'play' : 'pause') + ' demo recording');
    sync();
  }

  radios.forEach(function (radio) {
    radio.addEventListener('change', sync);
  });

  toggle.addEventListener('click', function () {
    setPaused(!paused);
  });

  reducedMotion.addEventListener('change', function (event) {
    setPaused(event.matches);
  });

  // Labels the control and starts the selected recording unless motion is
  // reduced.
  toggle.hidden = false;
  setPaused(paused);
})();
