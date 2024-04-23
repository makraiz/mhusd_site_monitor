Simple network monitoring project to test out Vizia's GUI framework for Rust.  

Monitors multiple IP addresses concurrently via frequent pings, with configurable payload size, timeouts, ping interval, and hot reload of sites file.  Supports IPv4 & IPv6 addresses.  

To use, create a file called 'sites.json' in the root directory of the executable.  Example format:
```
{
  "SiteName": "127.0.0.1",
  "SiteName2": "0:0:0:0:0:0:0:1"
}
```
