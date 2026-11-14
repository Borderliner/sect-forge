# Repository workflow

Commit and push completed changes to the branch's configured upstream unless the user asks to keep them local. Follow the repository's SemVer commit hook. Use local builds and tests for verification.

## Campaign testing

Skip startup movies and campaign intros on every interactive test launch. Use the local `zzz_sectforge_skip_intros.pack` movie pack, which overrides the relevant `videos_tables` entries. Keep the frontend leader's video references intact because the campaign loader expects them. Leave Spectra profiles alone.
