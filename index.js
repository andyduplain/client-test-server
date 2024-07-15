import express from "express";
const app = express();
import fs from "fs";
import parseRange from "range-parser";
import { fileURLToPath } from "url";
import { dirname } from "path";

const PORT = 1337;

const thisfile = fileURLToPath(import.meta.url);
const thisdir = dirname(thisfile);

app.get("/", function (req, res) {
  res.sendFile(thisdir + "/content/index.html");
});

app.get("/video", function (req, res) {
  console.log("Request: " + JSON.stringify(req.headers));

  const videoPath = thisdir + "/content/video.mp4";
  const videoSize = fs.statSync(videoPath).size;

  const ranges = parseRange(videoSize, req.headers.range, { combine: true });
  if (Array.isArray(ranges) && ranges.length == 1) {
    const range = ranges[0];
    const contentLength = range.end - range.start;
    const headers = {
      "Content-Range": `bytes ${range.start}-${range.end}/${videoSize}`,
      "Accept-Ranges": "bytes",
      "Content-Length": contentLength,
      "Content-Type": "video/mp4",
    };

    console.log("Response: " + JSON.stringify(headers));

    // HTTP Status 206 for Partial Content
    res.writeHead(contentLength == 0 ? 200 : 206, headers);

    const videoStream = fs.createReadStream(videoPath, {
      start: range.start,
      end: range.end,
    });
    videoStream.pipe(res);
  } else {
    res.status(400).send("Invalid RANGE header");
  }
});

app.listen(PORT, "0.0.0.0", function () {
  console.log(`Listening on port ${PORT}`);
});
