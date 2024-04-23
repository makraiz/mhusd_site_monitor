Simple network monitoring tool to test out Vizia's GUI framework for Rust.  I built this for work, but it is universal enough to be used anywhere you would want to monitor a bunch of IP addresses.  

Monitors multiple IP addresses concurrently via frequent pings, with configurable payload size, timeouts, ping interval, and hot reload of sites file.  Supports IPv4 & IPv6 addresses.  

To use, create a file called 'sites.json' in the root directory of the executable.  Example format:
```
{
  "SiteName": "127.0.0.1",
  "SiteName2": "0:0:0:0:0:0:0:1"
}
```
