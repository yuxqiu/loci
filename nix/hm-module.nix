{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.loci;

  # One entry per skill folder under share/loci/skills/ of package and plugins.
  # Reading the store path is import-from-derivation: the packages build at eval time.
  skills = lib.concatMap (
    pkg:
    let
      dir = "${pkg}/share/loci/skills";
    in
    lib.mapAttrsToList (name: _: {
      inherit name;
      source = "${dir}/${name}";
    }) (lib.filterAttrs (_: type: type == "directory") (builtins.readDir dir))
  ) ([ cfg.package ] ++ cfg.plugins);

  skillNames = map (s: s.name) skills;

  linksUnder =
    subdir:
    lib.concatMap (
      vault:
      map (s: {
        name = "${vault}/${subdir}/${s.name}";
        value = {
          inherit (s) source;
          recursive = true;
        };
      }) skills
    ) cfg.vaults;
in
{
  options.programs.loci = {
    enable = lib.mkEnableOption "loci";
    package = lib.mkOption {
      type = lib.types.package;
      default = (import ./package.nix { inherit pkgs; }).loci;
      defaultText = lib.literalExpression "loci from this flake";
      description = "The loci package.";
    };
    plugins = lib.mkOption {
      type = lib.types.listOf lib.types.package;
      default = [ ];
      example = lib.literalExpression "[ loci-learn ]";
      description = "Plugin packages to install; their skills are linked into each vault.";
    };
    vaults = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "notes" ];
      description = "Vault paths relative to $HOME.";
    };
    claudeSkills = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Also link the skills into each vault's .claude/skills.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = lib.allUnique skillNames;
        message = "programs.loci: duplicate skill names across package and plugins: ${toString skillNames}";
      }
    ];
    home.packages = [ cfg.package ] ++ cfg.plugins;
    home.file = lib.listToAttrs (
      linksUnder ".agents/skills" ++ lib.optionals cfg.claudeSkills (linksUnder ".claude/skills")
    );
  };
}
