# Prompts for the cut-out art

The first cast's sheets (the six workers, the manager and the pets) were drawn by Codex (`codex exec -s workspace-write`, its built-in `image_gen` tool, Codex 0.161.0, 2026-10-09) from the prompts below. The pilot sheet was saved as `pilot-sheet.png` and renamed `theo-sheet.png`. Cut the sheets with `uv run --script art/cast/cut.py`.

## Pilot (theo)

    Use your built-in image generation tool (image_gen) to create ONE image, then copy the generated PNG into this repository at `art/cast/raw/pilot-sheet.png`. Do not edit any other file. Do not write code.
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 30, light-brown skin, short curly black hair, forest-green button-up shirt with sleeves rolled to the forearms, navy trousers, brown leather shoes. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

## The other six workers and the manager

    Use your built-in image generation tool (image_gen) to create SIX separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. Every sheet must follow the same layout and rules; only the character changes. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/mara-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 42, East Asian woman, straight black chin-length bob, mustard-yellow knit sweater with long sleeves pushed to the forearms, grey wool trousers, black leather loafers. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/walt-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 55, white man, short grey hair, trimmed grey beard, thin metal glasses, navy cardigan over a white collared shirt with sleeves pushed to the forearms, beige chinos, brown leather shoes. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/ayo-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 26, Black woman, long box braids tied back in a low ponytail, rust-orange blouse with sleeves rolled to the forearms, dark-wash jeans, white canvas sneakers. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/dev-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 34, South Asian man, short neat black hair, clean-shaven, light-blue oxford shirt with sleeves rolled to the forearms, charcoal trousers, black leather shoes. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/fern-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 31, white woman, freckles, ginger-red hair in a high ponytail, plum-purple long-sleeve top pushed to the forearms, black trousers, brown ankle boots. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/boss-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office manager, about 52, Latino man, salt-and-pepper short hair, charcoal suit jacket and matching trousers, white shirt, dark-red tie, jacket sleeves ending at the wrist (forearm pieces show the jacket sleeve and shirt cuff), black oxford shoes. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

## Pets

    Use your built-in image generation tool (image_gen) to create ONE image, then copy the generated PNG into this repository at `art/cast/raw/pets-sheet.png`. Do not edit any other file. Do not write code.
    
    The image is a cut-out puppet PARTS SHEET for South Park-style cut-out animation, rendered REALISTICALLY: photographic / high-end 3D-render materials, real fur and feathers, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background. Every animal seen from the FRONT, facing the camera, flat-on like a paper cut-out.
    
    Draw these separate pieces, each completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 3-column by 3-row grid, each piece centred in its cell:
    
    Row 1 (a tabby cat, sitting):
    1. CAT BODY sitting upright, front view, with front legs and paws, NO head and NO tail (clean cut at the neck).
    2. CAT HEAD with ears, front view, eyes open, looking at the viewer.
    3. CAT TAIL alone, curving upward, the base of the tail at the bottom.
    
    Row 2 (a small brown-and-white terrier dog, sitting):
    4. DOG BODY sitting upright, front view, with front legs and paws, NO head and NO tail.
    5. DOG HEAD with floppy ears, front view, mouth slightly open, looking at the viewer.
    6. DOG TAIL alone, pointing upward, the base of the tail at the bottom.
    
    Row 3:
    7. OWL BODY (a small barn owl), front view, wings folded, feet visible, NO head.
    8. OWL HEAD, front view, big dark eyes, looking at the viewer.
    9. A SEASHELL: a single large spiral conch / whelk shell, pearly cream and orange, lying on its side, opening facing the viewer.
    
    All three animals at roughly the same size, each sitting animal about as tall as a seated person's torso.

## Tier workers for GitHub issue #14 (2026-11-20, codex-cli 0.161.0)

Drawn on 2026-11-20 by Codex (`codex exec -s workspace-write --skip-git-repo-check`, its built-in `image_gen` tool, codex-cli 0.161.0). 34 images in all: one pilot, 23 in the main run, four generated again and six wood swatches. Nothing was edited by hand. The prompts below are verbatim, as sent.

Four workers, one for each model tier, each in six builds from lean to buff, one build for each effort level (`minimal`, `low`, `medium`, `high`, `xhigh`, `max`). The sheets are `art/cast/raw/<worker>-<build>-sheet.png`.

| Worker | Models |
| --- | --- |
| kit | Haiku and Luna |
| nell | Sonnet and Terra |
| otto | Opus and Sol |
| vera | Fable and Astra |

Four sheets were generated again. `nell-high` came out with a hazy background that was not transparent, and the cutter refused it. `nell-xhigh`, `nell-max` and `vera-max` drew the stronger builds with a larger bust, not a stronger frame, so their second prompts say where strength shows. The first attempts are not in the repo.

The file names inside the prompts are the names the sheets were generated under. Afterwards each worker's six sheets were ordered by measured girth (the summed widths of torso, pelvis, arms and legs once the cutter has scaled the torso to 96 px) and renamed, so that every ladder runs from lean to buff. These sheets serve a neighbouring build from the one their prompt names. kit needed none.

| Worker | Generated as | Used as |
| --- | --- | --- |
| nell | `low` | `minimal` |
| nell | `minimal` | `low` |
| otto | `low` | `minimal` |
| otto | `minimal` | `low` |
| otto | `high` | `medium` |
| otto | `medium` | `high` |
| vera | `high` | `medium` |
| vera | `medium` | `high` |

The cutter takes each worker's head, closed-eyes head and chair from its `medium` sheet for all six builds, so the face is the same picture in every build. The other eight pieces come from the build's own sheet. Cut with `uv run --script art/cast/cut.py`, and check with `pnpm check:art`.

### Pilot (`kit-medium-sheet.png`)

    Use your built-in image generation tool (image_gen) to create ONE image, then copy the generated PNG into this repository at `art/cast/raw/kit-medium-sheet.png`. Do not edit any other file. Do not write code.
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 23, Southeast Asian man, short black hair with an undercut, clean-shaven, teal crew-neck sweatshirt with sleeves pushed to the forearms, light-grey chinos, white leather sneakers. Body build: average, healthy build. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

### kit, the other five builds

    Use your built-in image generation tool (image_gen) to create FIVE separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. Every sheet must follow the same layout and rules and show the SAME person in the SAME clothes; only the body build changes from sheet to sheet. If your image tool can take an earlier image as a reference, use the first sheet you made (or `art/cast/raw/kit-medium-sheet.png` if it already exists) as the reference for the face, hair, skin tone and clothes of the others. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/kit-minimal-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 23, Southeast Asian man, short black hair with an undercut, clean-shaven, teal crew-neck sweatshirt with sleeves pushed to the forearms, light-grey chinos, white leather sneakers. Body build: very slim, slight build: narrow shoulders, a narrow chest, thin arms and thin legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/kit-low-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 23, Southeast Asian man, short black hair with an undercut, clean-shaven, teal crew-neck sweatshirt with sleeves pushed to the forearms, light-grey chinos, white leather sneakers. Body build: slim, lean build: slender shoulders, arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/kit-high-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 23, Southeast Asian man, short black hair with an undercut, clean-shaven, teal crew-neck sweatshirt with sleeves pushed to the forearms, light-grey chinos, white leather sneakers. Body build: athletic, toned build: somewhat broader shoulders and chest, firm defined arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/kit-xhigh-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 23, Southeast Asian man, short black hair with an undercut, clean-shaven, teal crew-neck sweatshirt with sleeves pushed to the forearms, light-grey chinos, white leather sneakers. Body build: muscular build: broad shoulders, a deep chest, thick upper arms and forearms that fill the sleeves, strong thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/kit-max-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 23, Southeast Asian man, short black hair with an undercut, clean-shaven, teal crew-neck sweatshirt with sleeves pushed to the forearms, light-grey chinos, white leather sneakers. Body build: very muscular, bodybuilder build: very broad shoulders and chest, very thick upper arms and forearms that strain the sleeves, powerful thick thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

### nell

    Use your built-in image generation tool (image_gen) to create SIX separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. Every sheet must follow the same layout and rules and show the SAME person in the SAME clothes; only the body build changes from sheet to sheet. If your image tool can take an earlier image as a reference, use the first sheet you made (or `art/cast/raw/nell-medium-sheet.png` if it already exists) as the reference for the face, hair, skin tone and clothes of the others. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/nell-minimal-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: very slim, slight build: narrow shoulders, a narrow chest, thin arms and thin legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-low-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: slim, lean build: slender shoulders, arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-medium-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: average, healthy build. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-high-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: athletic, toned build: somewhat broader shoulders and chest, firm defined arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-xhigh-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: muscular build: broad shoulders, a deep chest, thick upper arms and forearms that fill the sleeves, strong thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-max-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: very muscular, bodybuilder build: very broad shoulders and chest, very thick upper arms and forearms that strain the sleeves, powerful thick thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

### otto

    Use your built-in image generation tool (image_gen) to create SIX separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. Every sheet must follow the same layout and rules and show the SAME person in the SAME clothes; only the body build changes from sheet to sheet. If your image tool can take an earlier image as a reference, use the first sheet you made (or `art/cast/raw/otto-medium-sheet.png` if it already exists) as the reference for the face, hair, skin tone and clothes of the others. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/otto-minimal-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 45, Middle Eastern man, short dark hair greying at the temples, short trimmed dark beard, burgundy merino sweater with sleeves pushed to the forearms, dark-grey trousers, dark-brown leather shoes. Body build: very slim, slight build: narrow shoulders, a narrow chest, thin arms and thin legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/otto-low-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 45, Middle Eastern man, short dark hair greying at the temples, short trimmed dark beard, burgundy merino sweater with sleeves pushed to the forearms, dark-grey trousers, dark-brown leather shoes. Body build: slim, lean build: slender shoulders, arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/otto-medium-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 45, Middle Eastern man, short dark hair greying at the temples, short trimmed dark beard, burgundy merino sweater with sleeves pushed to the forearms, dark-grey trousers, dark-brown leather shoes. Body build: average, healthy build. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/otto-high-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 45, Middle Eastern man, short dark hair greying at the temples, short trimmed dark beard, burgundy merino sweater with sleeves pushed to the forearms, dark-grey trousers, dark-brown leather shoes. Body build: athletic, toned build: somewhat broader shoulders and chest, firm defined arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/otto-xhigh-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 45, Middle Eastern man, short dark hair greying at the temples, short trimmed dark beard, burgundy merino sweater with sleeves pushed to the forearms, dark-grey trousers, dark-brown leather shoes. Body build: muscular build: broad shoulders, a deep chest, thick upper arms and forearms that fill the sleeves, strong thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/otto-max-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 45, Middle Eastern man, short dark hair greying at the temples, short trimmed dark beard, burgundy merino sweater with sleeves pushed to the forearms, dark-grey trousers, dark-brown leather shoes. Body build: very muscular, bodybuilder build: very broad shoulders and chest, very thick upper arms and forearms that strain the sleeves, powerful thick thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

### vera

    Use your built-in image generation tool (image_gen) to create SIX separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. Every sheet must follow the same layout and rules and show the SAME person in the SAME clothes; only the body build changes from sheet to sheet. If your image tool can take an earlier image as a reference, use the first sheet you made (or `art/cast/raw/vera-medium-sheet.png` if it already exists) as the reference for the face, hair, skin tone and clothes of the others. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/vera-minimal-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: very slim, slight build: narrow shoulders, a narrow chest, thin arms and thin legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/vera-low-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: slim, lean build: slender shoulders, arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/vera-medium-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: average, healthy build. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/vera-high-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: athletic, toned build: somewhat broader shoulders and chest, firm defined arms and legs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/vera-xhigh-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: muscular build: broad shoulders, a deep chest, thick upper arms and forearms that fill the sleeves, strong thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/vera-max-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: very muscular, bodybuilder build: very broad shoulders and chest, very thick upper arms and forearms that strain the sleeves, powerful thick thighs. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

### nell, generated again (`high`, `xhigh`, `max`)

    Use your built-in image generation tool (image_gen) to create THREE separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. Every sheet must follow the same layout and rules and show the SAME person in the SAME clothes as `art/cast/raw/nell-medium-sheet.png`, which already exists: use it as the reference image for the face, hair, skin tone and clothes if your image tool can take one. Only the body build changes from sheet to sheet. The background must be genuinely transparent with no haze, glow or colour left between the pieces. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/nell-high-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: athletic, toned build: somewhat broader shoulders and chest, firm defined arms and legs. Strength shows as broad shoulders, thick arms, a solid waist and strong legs. The chest is flat and athletic, like a rower's or a climber's, and the shirt lies flat over it. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-xhigh-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: muscular build: broad shoulders, a deep chest, thick upper arms and forearms that fill the sleeves, strong thighs. Strength shows as broad shoulders, thick arms, a solid waist and strong legs. The chest is flat and athletic, like a rower's or a climber's, and the shirt lies flat over it. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.
    
    
    ### Sheet `art/cast/raw/nell-max-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 33, Black woman, short natural afro, crisp white linen shirt with sleeves rolled to the forearms, olive-green trousers, tan leather shoes. Body build: very muscular, bodybuilder build: very broad shoulders and chest, very thick upper arms and forearms that strain the sleeves, powerful thick thighs. Strength shows as broad shoulders, thick arms, a solid waist and strong legs. The chest is flat and athletic, like a rower's or a climber's, and the shirt lies flat over it. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

### vera, generated again (`max`)

    Use your built-in image generation tool (image_gen) to create ONE image, then copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code. The sheet must show the SAME person in the SAME clothes as `art/cast/raw/vera-medium-sheet.png`, which already exists: use it as the reference image for the face, hair, skin tone and clothes if your image tool can take one. Only the body build differs. The background must be genuinely transparent with no haze, glow or colour left between the pieces. Keep alpha transparency.
    
    ### Sheet `art/cast/raw/vera-max-sheet.png`
    
    The image is a cut-out puppet PARTS SHEET for a South Park-style cut-out animation, but rendered REALISTICALLY: photographic / high-end 3D-render materials, real fabric folds, real skin, real hair, soft studio lighting from the upper left. Genuinely transparent background (preserve alpha). No text, no labels, no grid lines, no shadows on the background.
    
    The character: an adult office worker, about 58, white woman, silver-white hair in a sleek chin-length bob, black turtleneck with sleeves pushed to the forearms, black tailored trousers, black leather ankle boots. Body build: very muscular, bodybuilder build: very broad shoulders and chest, very thick upper arms and forearms that strain the sleeves, powerful thick thighs. Strength shows as broad shoulders, thick arms, a solid waist and strong legs. The chest is flat and athletic, like a rower's or a climber's, and the shirt lies flat over it. The build changes ONLY the girth (the width) of the torso, pelvis, arms and legs. The height of the character and the LENGTH of every piece are those of an average adult, exactly as for any other build, and the head, face, hair, skin tone and clothes are exactly the same person in every build. Natural adult proportions (NOT chibi, NOT big-headed). Everything seen from the FRONT, facing the camera directly, flat-on like a paper cut-out.
    
    Draw these separate pieces, each piece completely separated from every other piece by clear transparent space (at least 40 px gap, no overlaps, no touching), laid out as a 4-column by 3-row grid, each piece centred in its cell:
    
    Row 1:
    1. HEAD with neck, front view, eyes open, neutral friendly expression, looking at the viewer.
    2. The SAME head with neck, eyes closed.
    3. TORSO only: shirt from neck opening down to the belt, front view, NO head, NO arms (clean flat cut at the shoulders and armholes, like a cut-out).
    4. Office CHAIR BACKREST only, black mesh, front view.
    
    Row 2:
    5. LEFT UPPER ARM (the character's left, viewer's right): shoulder to elbow, sleeve, hanging straight down, vertical.
    6. RIGHT UPPER ARM: shoulder to elbow, sleeve, hanging straight down, vertical.
    7. LEFT FOREARM WITH HAND: elbow at the top, rolled-up sleeve edge, bare forearm, hand at the bottom with fingers slightly curled as if over a keyboard, palm facing down/away, hanging straight down, vertical.
    8. RIGHT FOREARM WITH HAND: same, mirrored.
    
    Row 3:
    9. LEFT LEG: hip to shoe, trousers, standing straight, vertical, shoe pointing toward the viewer.
    10. RIGHT LEG: same, mirrored.
    11. PELVIS / HIPS: belt and the top of the trousers, front view.
    12. (leave empty)
    
    All pieces at the same scale, as if cut from one life-size photo of the same person. Upper arm + forearm together should be about as long as the torso is tall.

## Desk woods for GitHub issue #14 (2026-11-20, codex-cli 0.161.0)

Six seamless grain swatches, 1254 by 1254, one for each tool's desk: maple (Claude Code), wenge (Codex), oak (Cursor), walnut (opencode), cherry (Antigravity) and pine (an unknown tool). They are in `art/wood/raw/`. `uv run --script art/wood/tile.py` makes the 192 px tiles the app ships in `src/assets/wood/`.

    Use your built-in image generation tool (image_gen) to create SIX separate images, one call per image, one after another. After each one, copy the generated PNG into this repository at the path named in its heading. Do not edit any other file. Do not write code.
    
    Every image is a SEAMLESS, TILEABLE wood grain texture swatch for a desk top, rendered realistically like a photograph of a planed and oiled board seen straight from above. Square image. The grain runs left to right. Flat, even lighting with no highlights, no shadows, no vignette. No knots, no board joints, no edges, no border, no text. The left edge must continue into the right edge and the top into the bottom, so the swatch can repeat as a pattern without a visible seam. Fully opaque.
    
    ### Swatch `art/wood/raw/maple.png`
    
    The wood is pale maple: light honey-cream, fine straight grain.
    
    ### Swatch `art/wood/raw/wenge.png`
    
    The wood is wenge: near-black brown with fine darker and slightly lighter straight streaks.
    
    ### Swatch `art/wood/raw/oak.png`
    
    The wood is oak: mid golden-brown with open, visible grain and soft cathedral figure.
    
    ### Swatch `art/wood/raw/walnut.png`
    
    The wood is walnut: mid-dark chocolate brown with flowing grain.
    
    ### Swatch `art/wood/raw/cherry.png`
    
    The wood is cherry: warm reddish mid-brown with fine smooth grain.
    
    ### Swatch `art/wood/raw/pine.png`
    
    The wood is pine: pale yellow with soft wide grain lines.
