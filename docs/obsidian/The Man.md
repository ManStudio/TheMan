---
tags:
  - project/the-man
---
With this project I want to make a decentralized communication system that can work on any type of network, the application made with this project should be able to work in hard times, there will not be any central server.

**THIS IS NOT A SPECIFICATION**, this will be the abstract idea of the project any structure in this will be abstract, so `time` type can be `u32` or `u256` can be seconds or nanoseconds.

The types notation is like [Zig](https://ziglang.org).

Any [[Raw Message]] or [[Raw Conversation]] will be known by their [[Ticket]] only the tickets and the [[Signed]] data will be needed to be stored on the disk.

This project in the beginning will be a simple chat/audio/video communication application similar to [Discord](https://discord.com/) but with not central server.

In the future will have a media discovery system, the [[Composite]] will be the base of any "edited" video or live.

The servers of this project will be similar to [CS](https://en.wikipedia.org/wiki/Counter-Strike_(video_game)) or [Minecraft](https://en.wikipedia.org/wiki/Minecraft) servers, in the sense that they will be a source of truth for them self, they will be external you should be able to talk to any of your friends without using a server, but servers could be use to find people, the same for Content Discovery.

The servers can also be miners meaning that they seeds data, your friend when you are offline can store a message to a server to be received by you when you came back online, but you and that friend needs to use the same server, you can be connected to any number of servers.

The server will store some conversation tails and provide the [[Signed]] Data for the [[Raw Conversation]] or [[Raw Message]].

[[The Man V1]]