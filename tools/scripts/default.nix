# One package per script, named after its file so every existing call site
# and muscle-memory invocation keeps working. The tmux ones gain a `tmux-`
# prefix: they were never on PATH before, and `layout.sh` is too generic to be.
#
# Left out of runtimeInputs on purpose, so they resolve from the caller's PATH:
# tmux, hyprctl, ydotool, powerprofilesctl, usbguard and systemd's tools all
# talk to a running server that must match their version, and sudo/pkexec are
# setuid wrappers a store path cannot provide.
{ pkgs }:
let
  inherit (pkgs) lib;

  sh =
    name: file: runtimeInputs:
    pkgs.writeShellApplication {
      inherit name runtimeInputs;
      text = builtins.readFile file;
    };

  py =
    path: runtimeInputs:
    pkgs.writers.writePython3Bin (baseNameOf path)
      {
        # Both are black's formatting, which flake8 disagrees with.
        flakeIgnore = [
          "E501" # 100 columns, not 79
          "E203" # whitespace before a slice colon
        ];
        makeWrapperArgs = lib.optionals (runtimeInputs != [ ]) [
          "--prefix"
          "PATH"
          ":"
          (lib.makeBinPath runtimeInputs)
        ];
      }
      # The writer adds its own; the file keeps one for editors and direct runs.
      (lib.removePrefix "#!/usr/bin/env python3\n" (builtins.readFile ./${path}));

  script = path: sh (baseNameOf path) ./${path};
  tmuxScript = name: sh "tmux-${name}" ./tmux/${name};

  scripts = with pkgs; {
    "add-icon.sh" = script "media/add-icon.sh" [
      imagemagick
      gtk3 # gtk-update-icon-cache
    ];
    "ani-to-png.sh" = script "media/ani-to-png.sh" [
      imagemagick
      xcursorgen
    ];
    "autoclicker.sh" = script "desktop/autoclicker.sh" [
      libnotify
      util-linux # setsid
      gawk
    ];
    "bulk-color-shift.sh" = script "media/bulk-color-shift.sh" [ scripts."color_shift.py" ];
    "caelestia-color.sh" = script "desktop/caelestia-color.sh" [ jq ];
    # nix-collect-garbage is left to PATH, to match the running daemon.
    "clean-system.sh" = script "system/clean-system.sh" [ rmlint ];
    "cliphist-remove-entry.sh" = script "desktop/cliphist-remove-entry.sh" [
      cliphist
      wofi
      libnotify
    ];
    "get-cursor-pos.sh" = script "desktop/get-cursor-pos.sh" [
      wl-clipboard
      wofi
    ];
    "git-auto.sh" = script "git/git-auto.sh" [ git ];
    "git-push-auto.sh" = script "git/git-push-auto.sh" [ git ];
    "mysql.sh" = script "system/mysql.sh" [ mariadb.client ];
    "power-mode.sh" = script "desktop/power-mode.sh" [
      wofi
      libnotify
    ];
    "push-batched.sh" = script "git/push-batched.sh" [
      git
      gawk
    ];
    "recolor-icons.sh" = script "media/recolor-icons.sh" [
      imagemagick
      gnused
    ];
    "remove-target-dirs.sh" = script "files/remove-target-dirs.sh" [ findutils ];
    "sckey.sh" = script "desktop/sckey.sh" [ screenkey ];
    "screenshot-focused-monitor.sh" = script "screenshot/screenshot-focused-monitor.sh" [
      jq
      grim
      wl-clipboard
      libnotify
      xdg-utils
    ];
    "screenshot-selection-copy.sh" = script "screenshot/screenshot-selection-copy.sh" [
      hyprshot
      libnotify
      xdg-utils
    ];
    "set-niceness.sh" = script "system/set-niceness.sh" [ procps ];
    "sqlite.sh" = script "system/sqlite.sh" [ sqlite ];
    "start-ssh.sh" = script "system/start-ssh.sh" [ openssh ];
    "tesseract-screenshot.sh" = script "screenshot/tesseract-screenshot.sh" [
      tesseract
      grim
      slurp
      wofi
      wl-clipboard
      libnotify
    ];
    "underscore-all-files-in-current-directory.sh" =
      script "files/underscore-all-files-in-current-directory.sh"
        [ ];
    "usb-device-connect.sh" = script "desktop/usb-device-connect.sh" [
      libnotify
      zenity
      util-linux # flock
      gawk
    ];
    "wayvnc-ghost-monitor.sh" = script "desktop/wayvnc-ghost-monitor.sh" [
      wayvnc
      procps # pkill
    ];

    "tmux-apply-colors.sh" = tmuxScript "apply-colors.sh" [ ];
    "tmux-layout.sh" = tmuxScript "layout.sh" [ ];
    "tmux-move-window-shift.sh" = tmuxScript "move-window-shift.sh" [ ];
    "tmux-new-named-window.sh" = tmuxScript "new-named-window.sh" [ ];
    "tmux-popup-move-pane.sh" = tmuxScript "popup-move-pane.sh" [
      scripts."caelestia-color.sh"
      scripts."tmux-popup-move-pane-picker.sh"
    ];
    "tmux-popup-move-pane-picker.sh" = tmuxScript "popup-move-pane-picker.sh" [
      scripts."caelestia-color.sh"
      fzf
    ];
    "tmux-popup-move-window.sh" = tmuxScript "popup-move-window.sh" [
      scripts."caelestia-color.sh"
      scripts."tmux-popup-move-window-picker.sh"
    ];
    "tmux-popup-move-window-picker.sh" = tmuxScript "popup-move-window-picker.sh" [
      scripts."caelestia-color.sh"
      fzf
      gawk
    ];
    "tmux-popup-lazygit.sh" = tmuxScript "popup-lazygit.sh" [ scripts."caelestia-color.sh" ];
    "tmux-popup-newwin.sh" = tmuxScript "popup-newwin.sh" [
      scripts."caelestia-color.sh"
      scripts."tmux-popup-newwin-picker.sh"
    ];
    "tmux-popup-newwin-picker.sh" = tmuxScript "popup-newwin-picker.sh" [
      scripts."caelestia-color.sh"
      scripts."tmux-new-named-window.sh"
      zoxide
      fzf
      findutils
    ];
    "tmux-popup-sessions.sh" = tmuxScript "popup-sessions.sh" [
      scripts."caelestia-color.sh"
      scripts."tmux-popup-sessions-picker.sh"
    ];
    "tmux-popup-sessions-picker.sh" = tmuxScript "popup-sessions-picker.sh" [
      scripts."caelestia-color.sh"
      fzf
      gawk
    ];
    "tmux-popup-switcher.sh" = tmuxScript "popup-switcher.sh" [
      scripts."caelestia-color.sh"
      scripts."tmux-popup-switcher-picker.sh"
    ];
    "tmux-popup-switcher-picker.sh" = tmuxScript "popup-switcher-picker.sh" [
      scripts."caelestia-color.sh"
      fzf
      findutils
    ];

    "color_shift.py" = py "media/color_shift.py" [ ];
    "compress_video.py" = py "media/compress_video.py" [ ffmpeg ];
    "count_lines.py" = py "files/count_lines.py" [ ];
  };
in
scripts
