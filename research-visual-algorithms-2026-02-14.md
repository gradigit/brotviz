# Research: Audio-Reactive Visual Algorithms for TUI Visualizer
Date: 2026-02-14
Depth: Quick

## Summary
Strong candidates for "more interesting" visuals are: reaction-diffusion, stable-fluid advection with vorticity confinement, fractal flames, sphere-traced SDF fractals, and modern beat/onset-driven transition logic. These are all compatible with a Metal compute pipeline and can be rendered into terminal targets after post-processing.

## Key Findings
1. **Stable fluid simulation + vorticity confinement** gives rich swirling, smoke-like motion and is a proven GPU technique.
2. **Gray-Scott reaction-diffusion** creates organic, morphing psychedelic textures with controllable regimes.
3. **Fractal flame (IFS variations + density accumulation)** is a classic psychedelic fractal method with huge visual diversity.
4. **Sphere tracing for implicit/fractal fields** supports deep, smooth zooming and cinematic structure changes.
5. **Improved procedural noise (Perlin, cellular/Worley style combinations)** is a good modulation substrate for motion, color, and deformation.
6. **Beat tracking / onset detection algorithms** enable smarter transitions (jump cuts on hard onsets, smoother morphs on sustained sections).
7. **projectM/Milkdrop compatibility model** confirms equation-driven presets + FFT/beat features are still the right core architecture.

## Sources
- NVIDIA GPU Gems Ch.38 (Fast Fluid Dynamics on GPU): https://developer.nvidia.com/gpugems/gpugems/part-vi-beyond-triangles/chapter-38-fast-fluid-dynamics-simulation-gpu
- Science 1993 (Gray-Scott pattern regimes via Pearson): https://doi.org/10.1126/science.261.5118.189
- Sphere tracing citation/DOI metadata: https://dblp.org/rec/journals/vc/Hart96
- Sphere tracing abstract summary + DOI: https://www.researchgate.net/publication/2792108_Sphere_Tracing_A_Geometric_Method_for_the_Antialiased_Ray_Tracing_of_Implicit_Surfaces
- projectM docs (Milkdrop-style preset system + FFT/beat pipeline): https://projectm-visualizer.org/docs
- projectM repository (preset/equation architecture): https://github.com/projectM-visualizer/projectm
- FLAM3 algorithm/site (fractal flame ecosystem): https://flam3.com/index_code
- FLAM3 source repository: https://github.com/scottdraves/flam3
- Perlin noise GPU implementation reference: https://developer.nvidia.com/gpugems/gpugems2/part-iii-high-quality-rendering/chapter-26-implementing-improved-perlin-noise
- Beat Tracking by Dynamic Programming (DOI metadata): https://www.researchgate.net/publication/249816846_Beat_Tracking_by_Dynamic_Programming
- Onset detection tutorial (DOI metadata): https://www.researchgate.net/publication/3334132_A_Tutorial_on_Onset_Detection_in_Music_Signals
