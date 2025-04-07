	owner: NodeId
	format: BlobFormat
	hash: [32]U8
The `hash` is a [BLAKE3](https://github.com/BLAKE3-team/BLAKE3) hash.

This is from [Iroh 0.31 BlobTicket](https://docs.rs/iroh-blobs/0.31.0/iroh_blobs/ticket/struct.BlobTicket.html)

If there will be a hash collision, that is to bad, IDK.

The hash will point to [[Signed]] and the data will be used only if the signature can be verified with the owner if not this is a invalid ticket. 