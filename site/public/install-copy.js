(function () {
  function announce(msg) {
    var live = document.getElementById('copy-live-region');
    if (!live) {
      live = document.createElement('div');
      live.id = 'copy-live-region';
      live.setAttribute('role', 'status');
      live.setAttribute('aria-live', 'polite');
      live.style.cssText =
        'position:absolute;width:1px;height:1px;padding:0;margin:-1px;overflow:hidden;' +
        'clip:rect(0 0 0 0);white-space:nowrap;border:0;';
      document.body.appendChild(live);
    }
    live.textContent = '';
    setTimeout(function () {
      live.textContent = msg;
    }, 30);
  }

  function flashState(btn, label, cls, ariaMsg, durationMs) {
    var original = btn.dataset.copyOriginalText || btn.textContent;
    btn.dataset.copyOriginalText = original;
    btn.textContent = label;
    btn.classList.add(cls);
    if (ariaMsg) announce(ariaMsg);
    setTimeout(function () {
      btn.textContent = original;
      btn.classList.remove(cls);
    }, durationMs);
  }

  var buttons = document.querySelectorAll('[data-copy-target]');
  buttons.forEach(function (btn) {
    btn.addEventListener('click', function () {
      var target = document.getElementById(btn.dataset.copyTarget);
      if (!target) return;
      var text = target.textContent.trim();

      if (!navigator.clipboard) {
        flashState(btn, 'select & copy', 'copy-failed', 'Clipboard unavailable — select the command manually.', 2400);
        return;
      }

      navigator.clipboard
        .writeText(text)
        .then(function () {
          flashState(btn, 'copied', 'copied', 'Copied install command.', 1400);
        })
        .catch(function () {
          flashState(btn, 'copy failed', 'copy-failed', 'Copy failed — select the command manually.', 2400);
        });
    });
  });
})();
