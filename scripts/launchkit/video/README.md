Remotion project for docs/demo/promo.mp4 and demo-full.mp4.

Setup (from the repo root): copy this folder to .launchkit/video, then
- public/clips/*.mp4 and src/events/*.events.json from docs/demo/raw/ (record.py output)
- public/audio/bgm.mp3: house-vibez.mp3 from the video-shotcraft skill (Mixkit Stock Music Free License)
- public/audio/click.wav: see the ffmpeg line in .launchkit/LOG.md (synthesised)
- public/logo.png: crates/kiln-app/res/kiln.ico, 256 px frame
Then `npm install`, `npx remotion render src/index.ts Promo out/promo-raw.mp4 --crf=17` (and `Full`),
and `python scripts/launchkit/finish_videos.py` for loudness and the watermark.
