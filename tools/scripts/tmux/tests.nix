# The tmux test suite as a flake check. The sandbox already keeps it away from
# any real tmux server; tests/*.sh isolate themselves too, for a run outside it.
{ pkgs, tools }:
pkgs.runCommand "tmux-tests"
  {
    nativeBuildInputs =
      (with pkgs; [
        tmux
        bash
        coreutils
        findutils
        gawk
        gnugrep
        gnused
        # What resurrect itself calls, beyond the above.
        procps
        gnutar
        gzip
        hostname
      ])
      ++ map (name: tools.${name}) [
        "tmux-claude-tag-pane.sh"
        "tmux-resurrect-save-claude.sh"
        "tmux-resurrect-resume-claude.sh"
        "tmux-resurrect-drop-sidebars.sh"
      ];
    # Its scripts call each other through #!/usr/bin/env, which the sandbox
    # lacks; a host has it, so only this copy needs the shebangs patched.
    RESURRECT = pkgs.runCommand "tmux-resurrect-sandboxed" { } ''
      cp -r ${pkgs.tmuxPlugins.resurrect}/share/tmux-plugins/resurrect $out
      chmod -R u+w $out
      patchShebangs $out
    '';
  }
  ''
    for t in ${./tests}/*.sh; do bash "$t"; done
    touch $out
  ''
