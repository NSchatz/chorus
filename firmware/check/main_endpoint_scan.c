/* The endpoint tree's safety scans, run over a repository root.
 *
 *   chorus-endpoint-scan [repository-root]
 *
 * Exit 0 when every rule holds, 1 when any finding fires, 2 when the unit list
 * itself is unreadable - which is a failure and not a clean scan, because a
 * check that cannot read its own list has checked nothing. */

#include "endpoint_scan.h"

#include <stdio.h>

int main(int argc, char **argv)
{
    const char *root = (argc > 1) ? argv[1] : ".";

    char list_path[CHORUS_SCAN_PATH * 2];
    snprintf(list_path, sizeof(list_path), "%s/firmware/endpoint-units.conf", root);

    chorus_unit_list_t list;
    char detail[CHORUS_SCAN_TEXT];
    detail[0] = '\0';
    if (chorus_unit_list_load(&list, list_path, detail, sizeof(detail)) != 0) {
        fprintf(stderr, "FAIL endpoint-unit-list-unreadable :: %s\n", detail);
        return 2;
    }

    chorus_scan_result_t result;
    chorus_endpoint_scan(root, &list, &result);
    chorus_scan_report(&result, chorus_scan_ok(&result) ? stdout : stderr);
    return chorus_scan_ok(&result) ? 0 : 1;
}
