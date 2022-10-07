## Amethyst Core

The Amethyst Core package contains the implementation of the Amethyst state transition logic, including data types and traits.
EVM execution is currently handled by `revm`, but we expect some divergence from that crate in future. In particular, we may be able to
take advantage of advice from the host to significantly speed up operations like sorting, and to use the limits from
(EIP-1985)[https://ethereum-magicians.org/t/eip-1985-sane-limits-for-certain-evm-parameters/3224] to reduce EVM overhead.
