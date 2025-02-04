---
tags:
  - project/the-man
---
This project is more like a sandbox for connecting projects. 
This project in the beginning will be a communication application like Discord but without Servers.
But in the future will have a content discovery system, any media stuff will be an [[Composite]]

## Stage 1

We need a way to send, receive and sync messages, this will start in the [[The Man Message Protocol]]
Stuff more complex for sending files or calls messages that starts with slash will be used.

## Commands
Commands are messages that starts with slash.

### Data
	/data <ticket> <alt>
### Auto
	/auto start <idx> <codec_name> <codec_settings>
	/auto play <idx> <ticket>
	/auto stop <idx>

Auto will be used for audio/video conversations.
the codec name `composite` is reserved.

The default audio codec will be [opus](https://opus-codec.org/) his name will be `opus`
 
