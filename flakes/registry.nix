{ pkgs }:
let
  lib = pkgs.lib;
  registryPath = ./flake-registry.json;
  registry =
    if builtins.pathExists registryPath then builtins.fromJSON (builtins.readFile registryPath) else
      throw "flakes/flake-registry.json is missing. Please provide a nix registry file.";

  allowedChars = lib.stringToCharacters "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._-";

  sanitize =
    str:
    lib.toLower (
      lib.concatStrings (
        map (
          c:
          if lib.elem c allowedChars then c else "-"
        ) (lib.stringToCharacters str)
      )
    );

  gitSlug =
    url:
    let
      withoutQuery = builtins.head (lib.splitString "?" url);
      withoutPrefix = lib.removePrefix "git+" withoutQuery;
      path = lib.splitString "/" withoutPrefix;
      base = lib.last path;
    in
    lib.removeSuffix ".git" base;

  registryId =
    source:
    let
      typeName = source.type;
    in
    if typeName == "git" then
      sanitize "git-${gitSlug source.url}"
    else if lib.elem typeName [ "github" "gitlab" "sourcehut" ] then
      sanitize "${typeName}-${source.owner}-${source.repo}"
    else
      sanitize "${typeName}-entry";

  registryIdFromTo =
    to:
    if (to ? owner) && (to ? repo) then sanitize "${to.type}-${to.owner}-${to.repo}" else sanitize "${to.type}-entry";

  flakeRef =
    source:
    let
      typeName = source.type;
    in
    if lib.elem typeName [ "github" "gitlab" "sourcehut" ] then
      "${typeName}:${source.owner}/${source.repo}"
    else if typeName == "git" then
      source.url
    else
      throw "Unsupported flake source type '${typeName}'";

  flakeRefFromRegistry =
    to:
    let
      baseUrl =
        if to.type == "git" then
          let
            url = lib.removePrefix "git+" to.url;
            joiner = if lib.hasInfix "?" url then "&" else "?";
          in
          url
          + (
            if to ? ref then "${joiner}ref=${to.ref}"
            else if to ? rev then "${joiner}rev=${to.rev}"
            else ""
          )
        else if lib.elem to.type [ "github" "gitlab" "sourcehut" ] then
          let
            joiner = "?";
          in
          "${to.type}:${to.owner}/${to.repo}"
          + (
            if to ? ref then "${joiner}ref=${to.ref}"
            else if to ? rev then "${joiner}rev=${to.rev}"
            else ""
          )
        else
          throw "Unsupported registry entry type '${to.type}'";
    in
    if lib.hasPrefix "git+" baseUrl || to.type == "git" then "git+" + (lib.removePrefix "git+" baseUrl) else baseUrl;

  registryEntriesFromFile =
    if registry ? flakes then
      map (
        entry: {
          id =
            if (entry ? from) && (entry.from ? id) then entry.from.id else registryIdFromTo entry.to;
          ref = flakeRefFromRegistry entry.to;
        }
      ) registry.flakes
    else
      throw "flakes/flake-registry.json does not contain a 'flakes' array.";

  combinedEntries =
    lib.attrValues (
      lib.foldl'
        (acc: entry: acc // { ${entry.id} = entry; })
        { }
        registryEntriesFromFile
    );
in
pkgs.runCommand "flake-registry.json" { nativeBuildInputs = [ pkgs.nix ]; } ''
  export NIX_CONFIG="extra-experimental-features = nix-command flakes"
  registry=$out
  : > "$registry"

  while IFS=$'\t' read -r id ref; do
    ${pkgs.nix}/bin/nix registry add --registry "$registry" "$id" "$ref"
  done <<'EOF'
  ${lib.concatStringsSep "\n" (map (entry: "${entry.id}\t${entry.ref}") combinedEntries)}
EOF
''
