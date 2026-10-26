# Changelog

## 0.1.0 (2026-10-26)


### Features

* add app bundle script and README ([04f7aad](https://github.com/Asugawara/unmouse/commit/04f7aad9c7e486be44411464720bbcf12bf9aa7f))
* add grid.extra_chars to make cells four times finer ([12c40c3](https://github.com/Asugawara/unmouse/commit/12c40c3f425f4824ee9adc7e69d088a9778d6faa))
* add grid.subdivide with mixed 5 and 6 character labels on half cells ([bfaf96c](https://github.com/Asugawara/unmouse/commit/bfaf96c5639b2d90e2a3ba11e9c2004c04b4e13a))
* add macOS overlay, event tap, mouse and CLI ([f6db9b3](https://github.com/Asugawara/unmouse/commit/f6db9b312f3b674ebb1711d4ae4072bdcffe3595))
* add pure grid logic with jkl; labels across monitors ([82e434c](https://github.com/Asugawara/unmouse/commit/82e434c796cf44e429ef65c5b887ec440ee4ace3))
* center labels in their cells by default ([ea1a2d6](https://github.com/Asugawara/unmouse/commit/ea1a2d685db13df84df34312153d41c799372059))
* choose the weight of the next-typed character with style.next_weight ([9912a59](https://github.com/Asugawara/unmouse/commit/9912a593388841e11d512377d35f9d33049a189f))
* close the overlay once a cell is determined unless grid.refine is set ([4f4b236](https://github.com/Asugawara/unmouse/commit/4f4b236a3a220e2cec4d5f945a3fa995bd55dee3))
* color the next character per key ([9d5e8f2](https://github.com/Asugawara/unmouse/commit/9d5e8f21bbdf31fbc017285659498ac61363ed74))
* draw the next character larger with style.next_scale ([ff765ba](https://github.com/Asugawara/unmouse/commit/ff765ba782b2252eb9c593cd95e067b6bfeb844e))
* fade the label background to the right and fit its height to the glyphs ([cc67da7](https://github.com/Asugawara/unmouse/commit/cc67da7a35b1ea6f726bb2aa9d3315f7c3d76313))
* generate the README screenshots offscreen with examples/screenshots ([aa52b37](https://github.com/Asugawara/unmouse/commit/aa52b376b364565a3bd8048c1e3fb3884582bf97))
* lower default label_size and edge_key_size to 10pt ([0386262](https://github.com/Asugawara/unmouse/commit/0386262d5214781f7a0bcd3d74140cb078edafde))
* move the cursor as soon as a cell is determined ([1f4b1fb](https://github.com/Asugawara/unmouse/commit/1f4b1fbd9bbbdb7b6810a4cab2ab5bb36035688a))
* move to the center of the typed range when the trigger closes the overlay ([6c64865](https://github.com/Asugawara/unmouse/commit/6c6486523f19c896600ccbbef68658e2d8ee411a))
* print the version with --version ([90d3a29](https://github.com/Asugawara/unmouse/commit/90d3a29137baa54f3c553c6f511ffde08da59fda))
* run as a LaunchAgent with install, uninstall and restart ([ce88d43](https://github.com/Asugawara/unmouse/commit/ce88d4303499d87bec7789e1676231cb69b619d6))
* start with a double tap of right Command ([1337b07](https://github.com/Asugawara/unmouse/commit/1337b07f4acdf2def23d9dcd687fba79ae09f243))
* use io.github.asugawara.unmouse as the bundle ID and launchd label ([6d48d77](https://github.com/Asugawara/unmouse/commit/6d48d7755a1805f1b2874bf2f113299b8a769b99))
* use vivid key colors and keep 512 cells by default ([8151d72](https://github.com/Asugawara/unmouse/commit/8151d72603a6f981543758aeffecbf3aaec6a6a7))


### Bug Fixes

* do not log the keycodes of keys passed through with --verbose ([3e1b767](https://github.com/Asugawara/unmouse/commit/3e1b767ea40e5c809396d118486a3bee28ae052f))
* measure double-tap timing with a monotonic clock ([7193bfc](https://github.com/Asugawara/unmouse/commit/7193bfcc2b45cf6f8d82e01a9928074370990ad6))
* reset the right Cmd double tap on mouse events ([ce7e18a](https://github.com/Asugawara/unmouse/commit/ce7e18a747825c2d4622d3fc5749a341ca3effa1))
