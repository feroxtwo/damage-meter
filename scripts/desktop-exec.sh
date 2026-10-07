#!/usr/bin/env bash
# Freedesktop string escaping followed by Exec quoting; no shell evaluation.
# https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html
desktop_exec_quote() {
    [[ "$1" != *$'\n'* && "$1" != *$'\r'* && "$1" != *$'\t'* && "$1" != *=* ]] || {
        echo "Installationspfad enthält ein für Desktop-Exec unzulässiges Zeichen." >&2; return 1;
    }
    local input=$1 quoted='' char i
    for ((i=0; i<${#input}; i++)); do
        char=${input:i:1}
        case "$char" in
            \\) quoted+='\\\\' ;;
            \") quoted+='\\"' ;;
            \$) quoted+='\\$' ;;
            \`) quoted+='\\`' ;;
            %) quoted+='%%' ;;
            *) quoted+="$char" ;;
        esac
    done
    printf '"%s"' "$quoted"
}
