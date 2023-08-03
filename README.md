# tSVoI
Simple Voice over Internet (console app)

## Why?
I always learn a new programming language by making some niche project, in this case I made a simple app for p2p voice calls with no webrtc only udp and tcp sockets from the rust standard library (with the help of some cargo packages of course).

The app handles signaling using a simple format that is sent as bytes across the network, then json events are shown to stdout to notify about new connections or changes (the file ```codes``` in this repo has the event and op codes used by the app). I decided to do it this way since [my first attempt](https://github.com/l1g4v/Savi) was full of things I was not able to solve (mostly related to UI). This way I can continue the project by "attaching" some UI to it in any other language since the thing can communicate via stdio.

## How to use
Get your input and output devices:
- Run ```./tSVoI 3```

Host the signaling server:
- Run the app with these arguments: ```./tSVoI 0 "<your username> <capture device name or '_' for default device> <playback device name>```
- The output will show something like this: ```{ "event_code": 0, "server_address": "<ipv6 address>", "server_key": "<base64 string>" }```

Connect to a signaling server:
- Run the app with these arguments: ```./tSVoI 1 "<your username> <server_address> <server_key> <capture device name> <playback device name>```