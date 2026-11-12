# mono-mini

A monorepo with three units, for unit discovery (WP1.1):

- `services/orders` (`deps.edn`): depends on `libs/common` through `:local/root`; has a `:test` alias.
- `services/billing` (`project.clj` with `:main`): a service.
- `libs/common` (`deps.edn`).

`orders` and `billing` both define a namespace called `config`, so symbol identity must include the unit.
