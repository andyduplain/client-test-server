# Client-Test-Server

A video-streaming web server to test Connect clients.

## Deploying

You will need a working Rust environment, then:

```
% cargo run --release
```

## Creating a long-running-instance

Add the file `~/.config/systemd/user/client-test-server.service` (change `ExecStart` location to suite your local environment):

```
[Unit]
Description=Client Test Server
After=network.target

[Service]
ExecStart=/home/andy/client-test-server/start.sh

[Install]
WantedBy=default.target
```

Run:

```
$ systemctl enable --user client-test-server.service
$ systemctl start --user client-test-server.service
```
