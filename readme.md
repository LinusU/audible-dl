# Audible DL

A tool to download Audible books when on a slow/unstable internet connection.

## Background

I've often have a hard time using the Download-button on Audible unless I'm on a quick and stable internet connection. Also, every time the download was interrupted, I had to start over from the beginning. This tool is a simple solution to that problem.

This tool can only download books that you have purchased from Audible, and should be able to download using their website.

## Installation

```bash
cargo install audible-dl
```

## Usage

You need to figure out two variables before you can use the tool:

1. Your Audible "customer ID". This can be found using the developer console in the network tab when trying to download an audiobook using the Audible website. The customer ID is a 60 character long string.
2. The book SKU. This can be found in the source code of the book page.

Once you have those two variables, you can run the tool like this:

```bash
audible-dl --customer_id <customer_id> <sku>
```
