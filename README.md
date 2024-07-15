# Connect Client Test Video Server.

## Prerequisites

- node.js
- npm

## Video

Get the video from [here](https://file-examples.com/wp-content/storage/2017/04/file_example_MP4_1280_10MG.mp4).

I also ensured that the video had a _fast start_ tag:

```
% ffmpeg -i video-orig.mp4 -c copy -map 0 -movflags faststart video.mp4
```

## Start Server

```
% npm install
% npm run start
```
