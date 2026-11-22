// Test observer for the native Turnstone process. Unix-domain custody IPC is local.
#include <sys/socket.h>
#include <sys/uio.h>
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdlib.h>
#include <string.h>
#include <netdb.h>
#include <errno.h>
static void record(const char *msg) {
    int saved = errno;
    const char *file = getenv("TURNSTONE_NET_RECEIPT");
    if (file) {
        int fd = open(file, O_WRONLY|O_CREAT|O_APPEND, 0600);
        if (fd >= 0) { write(fd, msg, strlen(msg)); close(fd); }
    }
    errno = saved;
}
static int inet_addr_family(const struct sockaddr *a) { return a && (a->sa_family == AF_INET || a->sa_family == AF_INET6); }
__attribute__((constructor)) static void start(void) { record("observer active\n"); }
static int obs_connect(int fd,const struct sockaddr *a,socklen_t n) {
    if (inet_addr_family(a)) record("connect\n"); return connect(fd,a,n);
}
static int obs_connectx(int fd,const sa_endpoints_t *e,sae_associd_t id,unsigned int flags,const struct iovec *v,unsigned int nv,size_t *len,sae_connid_t *cid) {
    if (e && inet_addr_family(e->sae_dstaddr)) record("connectx\n"); return connectx(fd,e,id,flags,v,nv,len,cid);
}
static ssize_t obs_sendto(int fd,const void *b,size_t n,int flags,const struct sockaddr *a,socklen_t l) {
    if (inet_addr_family(a)) record("sendto\n"); return sendto(fd,b,n,flags,a,l);
}
static ssize_t obs_sendmsg(int fd,const struct msghdr *msg,int flags) {
    if (msg && inet_addr_family(msg->msg_name)) record("sendmsg\n"); return sendmsg(fd,msg,flags);
}
static int obs_socket(int domain,int type,int protocol) {
    if (domain==AF_INET || domain==AF_INET6) record("internet socket\n"); return socket(domain,type,protocol);
}
static int obs_query(const char *n,const char *s,const struct addrinfo *h, struct addrinfo **r) {
    if(n) record("getaddrinfo\n"); return getaddrinfo(n,s,h,r);
}
#define HOOK(f,g) __attribute__((used,section("__DATA,__interpose"))) static const void* hook_##f[]={ (const void*)f,(const void*)g };
HOOK(obs_connect,connect)
HOOK(obs_connectx,connectx)
HOOK(obs_sendto,sendto)
HOOK(obs_sendmsg,sendmsg)
HOOK(obs_socket,socket)
HOOK(obs_query,getaddrinfo)
