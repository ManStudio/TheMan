---
tags:
  - project/the-man
  - protocol/tmm
---
This is a protocol used to send, receive and sync messages.

This is the abstract way on how the protocol works.
This protocol needs other protocol to fetch the data for the hashes, [Iroh Blobs](https://docs.rs/iroh-blobs/0.31.0/iroh_blobs/index.html) is used.
## Components
- [[Raw Conversation]]
- [[Raw Message]]
## Packets
This is what is sent to the other nodes.
### Welcome
This is treated as a sync packet.
When we receive this packet we will search in every [[Conversation]] that the other node is part of and send the last messages [[Ticket]]s
### Send Message
	ticket: Ticket

When we receive this packet we will try to download the ticket data, then we verify the [[#Signed]] data with the ticket owner, then we get the conversation from the message.conversation then the same process for verifying it and if the message owner is part of that conversation we will add the message to that [[Conversation]].