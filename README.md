# ping-proxy

The ping-proxy is a ping utility to send ICMP request packets through a proxy agent.

## usage

First, start the proxy agent.

```bash
guojing@dev$ sudo ./proxy
listen on port 2000 ...
```

Second, start a new terminal, and ping the host 10.0.0.50.

```bash
guojing@dev$ ./ping -r localhost -c 4 10.0.0.50
ping 10.0.0.50 (10.0.0.50) 64 bytes of data
64 bytes from 10.0.0.50: seq 1 ttl 64 time 0.469 ms
64 bytes from 10.0.0.50: seq 2 ttl 64 time 0.638 ms
64 bytes from 10.0.0.50: seq 3 ttl 64 time 0.633 ms
64 bytes from 10.0.0.50: seq 4 ttl 64 time 0.859 ms

--- 10.0.0.50 ping statistics ---
4 packets tx, 4 rx, 0 lost, 0 timeout, 0% packets loss
rtt min/max/avg 0.469/0.859/0.64975 ms
```

## Why ping-proxy

I encountered a case which the IoT devices only accept packet from the specified MAC address, because it use the hardware MAC filter function. So, I write the **ping-proxy** to ping those devices at any where. The **proxy** accept **ping** tasks and do the real ping works.

## License

This project is licensed under the [MIT license](https://opensource.org/licenses/MIT).

### Contribution

All contributions are welcomed!
