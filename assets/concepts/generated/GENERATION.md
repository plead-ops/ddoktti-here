# 착지·아야 / 하품 원화

2026-10-01, imagegen 스킬의 내장 image_gen 도구로 생성했다. 수작업 SVG 몸통 및 입 주변 덧칠을 대체한다. PNG의 원본 알파를 보존하고, 기존 VTracer 파이프라인에서 프레임별 측정과 공통 팔레트로 벡터화한다. 생성 결과는 자동으로 프롬프트의 해상도와 일치하지 않을 수 있어 실제 이미지 크기와 칸 경계를 측정했다.

## 착지·아야 (landing-hurt-v2.png)

참조: raster-before-svg/companion/emotions-v1.png, edge-v2.png.

```text
Use case: stylized-concept. Asset type: transparent desktop mascot animation sprite sheet, replacing an awkward hand-coded hurt/landing sequence. Input image 1 and image 2 are CHARACTER AND STYLE REFERENCES, not layouts to copy. Create ONE production-quality raster illustration sheet containing exactly SIX full-body poses of the SAME green round-headed robot mascot, in a precise 3-column by 2-row evenly spaced grid, 1536x1024 canvas with six 512x512 cells. Every pose stays entirely in its own cell with generous clear transparent margins, full antenna, hands, feet visible. Same face/glasses size and very short legs in every pose. Match the reference's appealing organic cartoon silhouette and natural suit tailoring closely: green rounded rectangle head, single stalk antenna with blue round tip, thick round blue glasses and white lenses, dark navy suit jacket with lapels and distinct curved jacket hem separate from trousers, white collar, bright blue tie, small gold lapel pin, rounded green mitten hands and tiny dark shoes. Clean confident smooth near-black outlines, clean flat fills suitable for high-quality tracing; avoid gradient shading, grain, jagged outlines, tiny color islands and excessive fold lines. Keep the original reference charm; do NOT make a stiff geometric SVG-looking body or assemble trapezoid limbs.
Read poses in row-major order:
1. gentle landing, knees softly bent, feet supporting body, both hands near knees, open wide eyes and tiny surprised mouth.
2. harder landing, squatting naturally, eyes squeezed shut behind the glasses, small cute 'ow' mouth; no injuries.
3. same squat, slight flinch, one hand bracing knee and other rubbing hip, squeezed-shut eyes.
4. next rubbing-hip animation frame, small change in elbow and wrist, head steady, same short-legged body and matching footprint.
5. easing pain, eyes starting to open, still crouched, hand settles at hip.
6. relief/recovery from crouch, eyes open with tiny relieved smile, knees still softly bent so transition to standing is gentle; no long legs.
All six are consistent front three-quarter views facing slightly right, natural character drawings rather than copied heads on new bodies. Align soles on the same baseline within each cell, head/glasses and antenna sizes identical, no unrelated props, no visible grid, no borders, no labels, no text, no ground shadow, truly transparent background. Palette green #00D66B, glasses/tie blue #075BD8, suit #26374F, outlines/shoes #081724, whites #FFFEFA, gold #F4C637, small mouth red #F32643 and inside #8C1831. Preserve transparent alpha.
```

## 졸기·하품 (sleep-yawn-v3.png)

참조: raster-before-svg/companion/emotions-v1.png, behaviors-v2.png.

```text
Use case: stylized-concept. Create ONE transparent raster sprite sheet for the existing green robot desktop pet from the reference images. Reference 1 supplies exact character identity and appealing organic cartoon drawing style. Reference 2 supplies the sleepy/yawn gesture sequence (third row) but NOT its imperfect mouth contours. Output exactly FOUR full-body drawings in a precise 2x2 grid on 1024x1024, each 512x512 cell with full antenna and feet inside generous transparent margins. Row-major: 1) drowsy standing with half-closed eyelids and a tiny relaxed open mouth; 2) cute natural yawn with smiling closed eyes, clean oval dark mouth, simple small red tongue, one rounded green mitten hand near but NOT intersecting the mouth; 3) gently dozing, eyes closed, head tilted slightly to one side, both hands resting down; 4) small sleepy head nod to the opposite side. Keep same lens/head dimensions, stubby short legs, round dark shoes and natural navy suit tailoring across all four, align soles to same baseline per cell. Preserve reference: rounded green head, single blue ball antenna, bold blue round glasses, white eye lenses (green half eyelids for sleepy), navy suit jacket with lapels, distinct curved hem separating jacket from trousers, white shirt collar, blue tie, small gold lapel pin. The mouth must be a single clean cartoon opening with a consistent near-black outline, dark inside and small red tongue: NO green ring, NO green band around the lips, NO cheek stripes, NO patched-over jaw/glasses, NO halo, NO odd floating fragments. Flat consistent colors #00D66B green, #075BD8 blue, #26374F suit, #081724 outline/shoes, #FFFEFA white, #F4C637 gold, #F32643 red, #8C1831 inside mouth. Smooth confident hand-drawn cartoon contours and natural clothing curves, not angular geometric SVG assembly. No gradients, glow, shading texture, ground shadow, labels, text, grid or background. Truly transparent background with crisp alpha.
```

## 철푸덕 엎어지기 (landing-flop-v3.png)

2026-10-01, 내장 image_gen 도구. 참조는 `raster-before-svg/companion/emotions-v1.png`.
3열×2행 중 앞의 5칸을 사용한다. 첫 두 포즈는 나머지와 같은 방향으로 보도록 벡터 변환 좌표에서 좌우 반전한다. 모두 렌즈 폭 기준 동일한 전신 배율을 적용하며 바닥 접점을 250에 맞춘다. 기존 `hurt-v1` 5프레임을 대체하며 가벼운 착지는 유지한다.

```text
Use case: stylized-concept. Production transparent desktop mascot sprite sheet. Reference image is CHARACTER IDENTITY and STYLE only. Create exactly FIVE whole-body consecutive cartoon poses on a 3-column x 2-row grid, each 512x512 cell on a 1536x1024 canvas, final bottom-right cell EMPTY. Same adorable green robot: oversized rounded green head, single antenna with blue tip, thick blue circular glasses with white lenses, expressive black pupils, natural navy suit with distinct jacket hem separate from tiny short trousers, white collar, blue tie and gold lapel pin, green mitten hands, tiny dark shoes. Match original character style, head size and short leg proportions. Sequence of a harmless clumsy belly flop, NOT painful injury: 1 top-left: has just flopped flat onto belly, head turned three-quarter toward viewer with both lenses visible, palms splayed gently forward and short legs trailing behind, eyes gently closed, small neutral smile, body clearly horizontal and resting on ground; 2 top-middle: same prone pose resting quietly with open slightly sheepish eyes, chin low, hands planted; 3 top-right: lifts head and chest using palms, lower belly and knees still on ground, tiny amused smile; 4 bottom-left: gets up into a short squat/kneel with one hand planted and other dusting front of jacket; 5 bottom-middle: stands up with a cheerful embarrassed smile brushing sleeve, short stubby legs and feet planted. Keep all poses same head/lens size and fully inside each cell, align lowest body contact with same baseline within each cell. Prone drawings must actually lie on their belly with torso horizontal, NOT sitting upright or clutching hurt hips. Full uncut antenna in every cell. Preserve organically drawn smooth outlines, flat clean color fills and rounded clothing folds. Uniform palette green #00D66B, blue #075BD8, suit #26374F, outline/shoes #081724, white #FFFEFA, gold #F4C637. No grimacing, painful expression, tears, bandages, stars, pain lines, ground shadows, text, borders, grid, props, glow, gradients or background. Truly transparent alpha. Generous transparent gaps between poses. Draw original appealing raster illustrations for later vector tracing; no stiff geometric assembled SVG look.
```

쓰다듬기 반응은 기존 승인된 `emotions-v1` 8·9번 원화를 재사용해 눈을 감고 미소 짓는다. 새 이미지 생성물로 표기하지 않는다.
