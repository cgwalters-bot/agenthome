# Code navigation

Before reading an unfamiliar source file, use `ast-outline` for its skeleton,
then `ast-outline show` for the symbol you need. Before searching for a
definition or its callers, use `ast-outline` and `ast-grep`, not text grep.
Use the `skills/ast-explore` skill. Fall back to `rg` and scoped reading only
when structural tools do not support the language or cannot parse the region.

Run `just check` after changing the installer. `just check-install` performs
the opt-in network installation twice; it is not part of the default recipe.
